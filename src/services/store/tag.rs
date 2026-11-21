use gpui_kit::*;

use crate::AppState;
use crate::database::entities::{TagNamespace, scan_source};
use crate::database::storage::scan_source as scan_source_store;
use crate::database::storage::tag as tag_store;
use crate::services::MacroTag;
use crate::services::macrotag::SourceMeta;

pub struct TagStore {
    pub cached_tags: Vec<MacroTag>,
    pub source_tags: Vec<MacroTag>,
}

impl TagStore {
    pub fn new() -> Self {
        Self {
            cached_tags: Vec::new(),
            source_tags: Vec::new(),
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let (items, sources) = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        (
                            tag_store::list_candidates(&db, &[TagNamespace::User], 50).await,
                            scan_source_store::list_scans(&db).await,
                        )
                    })
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(tags) = items {
                    this.cached_tags = tags
                        .into_iter()
                        .map(|t| MacroTag::Standard(t.into()))
                        .collect();
                }
                if let Ok(sources) = sources {
                    this.set_sources(&sources, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Rebuild the `From` tags from the registered scan sources.
    pub fn set_sources(&mut self, sources: &[scan_source::Model], cx: &mut Context<Self>) {
        self.source_tags = sources
            .iter()
            .map(|s| MacroTag::From(SourceMeta::from(s)))
            .collect();
        cx.notify();
    }

    pub fn search(&self, query: &str) -> Vec<MacroTag> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.cached_tags.clone();
        }
        self.cached_tags
            .iter()
            .filter(|t| t.display_name().to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn find_or_create_then<F>(
        &mut self,
        namespace: TagNamespace,
        name: &str,
        then: F,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(MacroTag, &mut App) + 'static,
    {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let name_str = name.trim().to_string();

        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(tag_store::find_or_create(&db, namespace, name_str))
                })
                .await;

            if let Ok(model) = res {
                this.update(cx, |this, cx| {
                    let mtag = MacroTag::Standard(model.into());
                    if !this.cached_tags.contains(&mtag) {
                        this.cached_tags.push(mtag.clone());
                    }
                    cx.notify();
                    then(mtag, cx);
                })
                .ok();
            }
        })
        .detach();
    }
}
