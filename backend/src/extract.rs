//! OOXML 纯文本抽取（服务端）：给 MCP 的 doc_text 工具用。
//! 只做「拿到可读文本」，不做样式还原；输出上限 MAX_CHARS，防止超大文档撑爆上下文。

/// 抽取文本的最大字符数
pub const MAX_CHARS: usize = 200_000;

pub fn extract_text(ext: &str, bytes: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "文件不是有效的 OOXML 压缩包".to_string())?;

    let out = match ext {
        "docx" => {
            let xml = read_entry(&mut archive, "word/document.xml")?;
            let xml = xml.replace("</w:p>", "</w:p>\n");
            strip_tags(&xml)
        }
        "xlsx" => {
            let xml = read_entry(&mut archive, "xl/sharedStrings.xml")?;
            collect_tag_texts(&xml, "t").join("\n")
        }
        "pptx" => {
            let mut names: Vec<String> = archive
                .file_names()
                .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
                .map(str::to_string)
                .collect();
            names.sort_by_key(|n| {
                n.rsplit_once("slide")
                    .and_then(|(_, r)| r.split_once('.'))
                    .and_then(|(num, _)| num.parse::<u64>().ok())
                    .unwrap_or(0)
            });
            let mut parts = Vec::new();
            for name in names {
                let xml = read_entry(&mut archive, &name)?;
                parts.push(collect_tag_texts(&xml, "t").join(" "));
            }
            parts.join("\n")
        }
        other => return Err(format!("暂不支持抽取 .{other} 的文本")),
    };

    let collapsed = out.split('\n').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    if collapsed.chars().count() > MAX_CHARS {
        Ok(collapsed.chars().take(MAX_CHARS).collect())
    } else {
        Ok(collapsed)
    }
}

fn read_entry(archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| format!("压缩包内未找到 {name}，可能不是标准 OOXML"))?;
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut file, &mut buf).map_err(|e| format!("读取 {name} 失败：{e}"))?;
    Ok(buf)
}

/// 收集 `<tag ...>text</tag>` 的文本（手工扫描，足够对付 OOXML 的规整输出）。
fn collect_tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after_tag = &rest[start + open.len()..];
        let Some(gt) = after_tag.find('>') else { break };
        let body = &after_tag[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        out.push(strip_tags(&body[..end]));
        rest = &body[end + close.len()..];
    }
    out
}

/// 去掉所有 XML 标签并解码实体。
fn strip_tags(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_decodes() {
        let xml = "<w:p><w:t>A &amp; B</w:t></w:p><w:p><w:t>&lt;C&gt;</w:t></w:p>";
        let xml = xml.replace("</w:p>", "</w:p>\n");
        assert_eq!(strip_tags(&xml).trim(), "A & B\n<C>");
    }

    #[test]
    fn collects_tag_texts() {
        let xml = r#"<sst><si><t>Hello</t></si><si><t><r>World</r></t></si></sst>"#;
        assert_eq!(collect_tag_texts(xml, "t"), vec!["Hello", "World"]);
    }

    #[test]
    fn extracts_docx_text() {
        // 用 zip 写一个最小 docx 再抽取
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("word/document.xml", opts).unwrap();
            let xml = "<?xml version=\"1.0\"?><w:document><w:body><w:p><w:t>\u{7b2c}\u{4e00}\u{6bb5}</w:t></w:p><w:p><w:t>Second</w:t></w:p></w:body></w:document>";
            std::io::Write::write_all(&mut w, xml.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let text = extract_text("docx", &bytes).unwrap();
        assert_eq!(text, "\u{7b2c}\u{4e00}\u{6bb5}\nSecond");
    }
}
