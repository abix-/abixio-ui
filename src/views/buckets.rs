use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn browse_view(&self) -> Element<Message> {
        let bucket_panel = self.bucket_list_view();
        let object_panel = self.object_list_view();

        row![container(bucket_panel).width(180), object_panel,]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn bucket_list_view(&self) -> Element<Message> {
        let mut col = column![text("Buckets").size(14), iced::widget::rule::horizontal(1)]
            .spacing(4)
            .padding(8);

        if self.loading_buckets {
            col = col.push(text("Loading...").size(12));
        }

        if let Some(Ok(buckets)) = &self.buckets {
            for bucket in buckets {
                let is_selected = self.selected_bucket.as_deref() == Some(&bucket.name);
                let name = bucket.name.clone();
                col = col.push(
                    button(text(&bucket.name).size(12))
                        .width(Length::Fill)
                        .style(if is_selected {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(Message::SelectBucket(name)),
                );
            }
        }

        if let Some(Err(e)) = &self.buckets {
            col = col.push(text(format!("Error: {}", e)).size(11));
        }

        col = col.push(iced::widget::rule::horizontal(1));
        col = col.push(
            row![
                text_input("new bucket", &self.new_bucket_name)
                    .on_input(Message::NewBucketNameChanged)
                    .size(11),
                button(text("+").size(12)).on_press(Message::CreateBucket),
            ]
            .spacing(4),
        );

        scrollable(col).height(Length::Fill).into()
    }
}
