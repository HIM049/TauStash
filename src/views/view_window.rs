use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::database::entities::collection;
use crate::views::generic_view::GenericView;

/// A standalone window showing one collection, closed once the collection is gone.
pub struct ViewWindow {
    collection_id: Uuid,
    name: SharedString,
    view: Entity<GenericView>,
}

impl ViewWindow {
    pub fn new(model: &collection::Model, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let collections = cx.global::<AppState>().collections.clone();
        cx.observe_in(&collections, window, |this, collections, window, cx| {
            match collections
                .read(cx)
                .collections
                .iter()
                .find(|c| c.id == this.collection_id)
            {
                Some(model) => {
                    this.name = model.name.clone().into();
                    cx.notify();
                }
                None => window.remove_window(),
            }
        })
        .detach();

        let view = cx.new(|cx| GenericView::from_collection(model, window, cx));

        Self {
            collection_id: model.id,
            name: model.name.clone().into(),
            view,
        }
    }
}

impl Render for ViewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .child(
                TitleBar::new().child(
                    div()
                        .h_flex()
                        .w_full()
                        .justify_center()
                        .text_xs()
                        .font_semibold()
                        .child(self.name.clone()),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .bg(cx.theme().secondary)
                    .text_color(cx.theme().foreground)
                    .child(self.view.clone()),
            )
    }
}
