use systemless_catalogue_tools::catalogue_tools::{catalogue, Catalogue, Document, Kind};

pub fn catalogue() -> Catalogue {
    let (game, markdown) =
        catalogue::parse_document(include_str!("../fixtures/sample.md")).unwrap();
    let mut application = game.clone();
    application.id = "sample-app".into();
    application.kind = Kind::Application;
    application.title = "Sample Application".into();
    application.route = None;
    let documents = [application, game]
        .into_iter()
        .map(|entry| Document {
            original: catalogue::serialize_document(&entry, &markdown).unwrap(),
            path: format!("catalogue/{}.md", entry.id).into(),
            entry,
            markdown: markdown.clone(),
        })
        .collect();
    Catalogue {
        config: systemless_catalogue_tools::catalogue_tools::Config::default(),
        documents,
        root: ".".into(),
    }
}
