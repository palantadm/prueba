//! XML parsing shared by the importers. Real QLC+ files start with `<!DOCTYPE FixtureDefinition>` (or `Workspace`),
//! which roxmltree refuses by default; allowing the DTD is safe here because roxmltree limits entity expansion.

use roxmltree::{Document, Error, ParsingOptions};

pub fn parse(xml: &str) -> Result<Document<'_>, Error> {
    Document::parse_with_options(xml.trim_start_matches('\u{feff}'), ParsingOptions { allow_dtd: true, ..Default::default() })
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepts_a_doctype_and_refuses_entity_bombs() {
        assert!(super::parse("<?xml version=\"1.0\"?>\n<!DOCTYPE FixtureDefinition>\n<FixtureDefinition xmlns=\"http://www.qlcplus.org/FixtureDefinition\"/>").is_ok());
        assert!(super::parse("\u{feff}<a/>").is_ok());
        let mut bomb = String::from("<!DOCTYPE a [<!ENTITY e0 \"aaaaaaaaaa\">");
        for i in 1..12 {
            bomb.push_str(&format!("<!ENTITY e{i} \"{}\">", format!("&e{};", i - 1).repeat(10)));
        }
        bomb.push_str("]><a>&e11;</a>");
        assert!(super::parse(&bomb).is_err());
    }
}
