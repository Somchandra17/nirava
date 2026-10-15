//! Source-span edits preserve all bytes outside the selected settings.
use crate::OutputState;
use anyhow::{Result,bail};
use kdl::{KdlDocument,KdlNode};

fn parse(text:&str)->Result<KdlDocument>{Ok(KdlDocument::parse_v1(text).or_else(|_|KdlDocument::parse_v2(text))?)}
fn matches(node:&KdlNode,output:&OutputState)->bool {
    node.name().value()=="output"&&node.get(0).and_then(|v|v.as_string()).is_some_and(|name|name==output.connector||name==output.identity)
}
fn span(node:&KdlNode)->std::ops::Range<usize>{let s=node.span();s.offset()..s.offset()+s.len()}

pub fn patch_outputs(text:&str,outputs:&[OutputState])->Result<String> {
    let doc=parse(text)?;
    let mut edits:Vec<(std::ops::Range<usize>,String)>=vec![];
    let mut appended=String::new();
    for output in outputs {
        let candidates=doc.nodes().iter().filter(|n|matches(n,output)).collect::<Vec<_>>();
        if candidates.len()>1{bail!("Multiple output definitions match {}; select a single source file",output.connector);}
        let Some(node)=candidates.first() else {appended.push_str(&new_output(output));continue;};
        let body=node.children().ok_or_else(||anyhow::anyhow!("Output block has no body"))?;
        let properties=[
            ("mode",format!("mode \"{}x{}@{:.3}\"",output.mode.width,output.mode.height,output.mode.refresh_millihz as f64/1000.0)),
            ("scale",format!("scale {}",output.scale)),
            ("position",format!("position x={} y={}",output.x,output.y)),
            ("transform",format!("transform \"{}\"",transform(&output.transform)?)),
            ("off",if output.enabled{String::new()}else{"off".into()}),
            ("variable-refresh-rate",if output.vrr_enabled{"variable-refresh-rate".into()}else{String::new()}),
        ];
        let mut insertions=String::new();
        for (name,replacement) in properties {
            let existing=body.nodes().iter().filter(|n|n.name().value()==name).collect::<Vec<_>>();
            if existing.len()>1{bail!("Duplicate {name} settings in {}",output.connector);}
            if let Some(child)=existing.first(){edits.push((span(child),replacement));}
            else if !replacement.is_empty(){insertions.push_str(&format!("\n    {replacement}\n"));}
        }
        if !insertions.is_empty(){let at=body.span().offset()+body.span().len();edits.push((at..at,insertions));}
    }
    edits.sort_by_key(|(range,_)|std::cmp::Reverse(range.start));
    let mut edited=text.to_string();
    for (range,replacement) in edits {
        if range.end>edited.len()||!edited.is_char_boundary(range.start)||!edited.is_char_boundary(range.end){bail!("Invalid parser span; configuration not edited");}
        edited.replace_range(range,&replacement);
    }
    edited.push_str(&appended);parse(&edited)?;Ok(edited)
}

pub fn extract_outputs(text:&str,outputs:&[OutputState])->Result<(String,String)> {
    let doc=parse(text)?;let mut remove=vec![];let mut extracted=String::new();
    for node in doc.nodes(){if outputs.iter().any(|o|matches(node,o)){let range=span(node);extracted.push_str(&text[range.clone()]);extracted.push('\n');remove.push(range);}}
    remove.sort_by_key(|r|std::cmp::Reverse(r.start));let mut root=text.to_string();for range in remove{root.replace_range(range,"");}
    let managed=patch_outputs(&extracted,outputs)?;
    root.push_str("\n// Display settings managed by Plain; use Restore Integration to undo.\ninclude \"plain-displays.kdl\"\n");
    parse(&root)?;Ok((root,managed))
}

fn transform(value:&str)->Result<&'static str>{Ok(match value {
    "Normal"|"normal"=>"normal","90"=>"90","180"=>"180","270"=>"270","Flipped"|"flipped"=>"flipped",
    "Flipped90"|"flipped-90"=>"flipped-90","Flipped180"|"flipped-180"=>"flipped-180","Flipped270"|"flipped-270"=>"flipped-270",_=>bail!("Unknown transform"),
})}
fn new_output(o:&OutputState)->String {
    let identity=serde_json::to_string(if o.identity.is_empty(){&o.connector}else{&o.identity}).unwrap_or_default();
    format!("\noutput {identity} {{\n    {}\n    mode \"{}x{}@{:.3}\"\n    scale {}\n    position x={} y={}\n    transform \"{}\"\n    {}\n}}\n",if o.enabled{""}else{"off"},o.mode.width,o.mode.height,o.mode.refresh_millihz as f64/1000.0,o.scale,o.x,o.y,transform(&o.transform).unwrap_or("normal"),if o.vrr_enabled{"variable-refresh-rate"}else{""})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn output()->OutputState{let mut o=crate::Snapshot::demo().displays.outputs.remove(0);o.connector="DP-5".into();o.identity="Example Screen Test".into();o}
    #[test]
    fn preserve_unrelated_text_inline_comments_and_unicode()->Result<()> {
        let text="// 日本語 comment\noutput \"DP-5\" {\n    // keep this\n    mode \"1920x1080@60\" // handmade\n    scale 1.0\n    focus-at-startup\n}\nbinds { Mod+Return { spawn \"kitty\"; }; }\n";
        let edited=patch_outputs(text,&[output()])?;
        assert!(edited.starts_with("// 日本語 comment\n"));assert!(edited.contains("// handmade"));
        assert!(edited.contains("focus-at-startup"));assert!(edited.ends_with("binds { Mod+Return { spawn \"kitty\"; }; }\n"));
        assert!(edited.contains("3440x1440@160.000"));Ok(())
    }
    #[test]
    fn migration_preserves_non_display_sections()->Result<()> {
        let text="input { keyboard { xkb { layout \"us\"; }; }; }\noutput \"DP-5\" { scale 1; }\n";
        let (root,managed)=extract_outputs(text,&[output()])?;
        assert!(root.starts_with("input { keyboard { xkb { layout \"us\"; }; }; }\n"));
        assert!(root.contains("include \"plain-displays.kdl\""));assert!(managed.contains("output \"DP-5\""));Ok(())
    }
    #[test]
    fn duplicate_definitions_are_refused() {
        assert!(patch_outputs("output \"DP-5\" {}\noutput \"DP-5\" {}\n",&[output()]).is_err());
    }
}
