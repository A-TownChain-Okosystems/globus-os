//! Media-library metadata, collections, search and deterministic ordering.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Reihenfolge, in der die Bibliothek sortiert wird.
pub enum SortOrder {
    /// Titel aufsteigend.
    TitleAscending,
    /// Titel absteigend.
    TitleDescending,
    /// Neueste zuerst.
    Newest,
    /// Aelteste zuerst.
    Oldest,
    /// Kleinste zuerst.
    SizeAscending,
    /// Groesste zuerst.
    SizeDescending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art einer Mediendatei nach MIME.
pub enum MimeType {
    /// Ton.
    Audio,
    /// Video.
    Video,
    /// Bild.
    Image,
    /// Unbekannt.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Was die Sammlung ueber eine Mediendatei kennt.
pub struct MediaMetadata {
    /// Kennung der Datei.
    pub id: u64,
    /// Pfad oder URI.
    pub uri: String,
    /// Anzeigetitel.
    pub title: String,
    /// Art der Datei.
    pub mime: MimeType,
    /// Groesse in Byte.
    pub size_bytes: u64,
    /// Aenderungszeitpunkt in Millisekunden seit Epoche.
    pub modified_unix_ms: u64,
    /// Optionaler Fingerabdruck des Inhalts.
    pub content_hash: Option<[u8; 32]>,
    /// Dauer in Millisekunden, falls bekannt.
    pub duration_ms: Option<u64>,
    /// Breite in Pixel, falls bekannt.
    pub width: Option<u32>,
    /// Hoehe in Pixel, falls bekannt.
    pub height: Option<u32>,
}

#[derive(Debug, Default)]
/// Sammlung mit Favoriten, Alben und Suche.
pub struct MediaCatalog {
    items: Vec<MediaMetadata>,
    favorites: Vec<u64>,
    albums: Vec<(String, Vec<u64>)>,
}

impl MediaCatalog {
    /// Nimmt einen Eintrag auf oder ersetzt ihn anhand der Kennung.
    pub fn upsert(&mut self, item: MediaMetadata) {
        self.items.retain(|x| x.id != item.id);
        self.items.push(item);
    }
    /// Alle Eintraege.
    pub fn items(&self) -> &[MediaMetadata] {
        &self.items
    }
    /// Markiert einen Eintrag als Favorit oder hebt die Markierung auf.
    pub fn set_favorite(&mut self, id: u64, favorite: bool) {
        self.favorites.retain(|x| *x != id);
        if favorite && self.items.iter().any(|x| x.id == id) {
            self.favorites.push(id);
        }
    }
    /// True, wenn der Eintrag als Favorit markiert ist.
    pub fn is_favorite(&self, id: u64) -> bool {
        self.favorites.contains(&id)
    }
    /// Fuegt einen Eintrag einem Album hinzu.
    pub fn add_to_album(&mut self, name: impl Into<String>, id: u64) {
        let name = name.into();
        if let Some((_, ids)) = self.albums.iter_mut().find(|(n, _)| *n == name) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        } else {
            self.albums.push((name, vec![id]));
        }
    }
    /// Eintraege, deren Titel die Suche enthaelt.
    pub fn search(&self, query: &str) -> Vec<&MediaMetadata> {
        let q = query.to_ascii_lowercase();
        self.items
            .iter()
            .filter(|x| {
                x.title.to_ascii_lowercase().contains(&q) || x.uri.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }
    /// Eintraege in der gewuenschten Reihenfolge.
    pub fn sorted(&self, order: SortOrder) -> Vec<&MediaMetadata> {
        let mut out: Vec<_> = self.items.iter().collect();
        match order {
            SortOrder::TitleAscending => out.sort_by(|a, b| a.title.cmp(&b.title)),
            SortOrder::TitleDescending => out.sort_by(|a, b| b.title.cmp(&a.title)),
            SortOrder::Newest => out.sort_by_key(|x| std::cmp::Reverse(x.modified_unix_ms)),
            SortOrder::Oldest => out.sort_by_key(|x| x.modified_unix_ms),
            SortOrder::SizeAscending => out.sort_by_key(|x| x.size_bytes),
            SortOrder::SizeDescending => out.sort_by_key(|x| std::cmp::Reverse(x.size_bytes)),
        };
        out
    }
    /// Gruppen von Eintraegen mit gleichem Inhaltsfingerabdruck.
    pub fn duplicate_ids(&self) -> Vec<Vec<u64>> {
        let mut groups = Vec::new();
        for item in &self.items {
            if let Some(hash) = item.content_hash {
                let mut ids = vec![item.id];
                for other in &self.items {
                    if other.id != item.id
                        && other.content_hash == Some(hash)
                        && !ids.contains(&other.id)
                    {
                        ids.push(other.id);
                    }
                }
                if ids.len() > 1
                    && !groups
                        .iter()
                        .any(|g: &Vec<u64>| g.iter().any(|id| ids.contains(id)))
                {
                    groups.push(ids);
                }
            }
        }
        groups
    }
}

/// Art der Datei anhand der Endung.
pub fn mime_from_extension(path: &str) -> MimeType {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" | "flac" | "opus" | "aac" => MimeType::Audio,
        "mp4" | "mkv" | "webm" | "mov" => MimeType::Video,
        "jpg" | "jpeg" | "png" | "webp" | "avif" => MimeType::Image,
        _ => MimeType::Unknown,
    }
}
