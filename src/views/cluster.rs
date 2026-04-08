use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::app::{App, Message};

impl App {
    pub fn cluster_view(&self) -> Element<'_, Message> {
        let mut layout = column![].spacing(8).padding(12).width(Length::Fill);

        layout = layout.push(
            row![
                text("Cluster").size(18),
                button(text("refresh").size(10))
                    .style(button::secondary)
                    .on_press(Message::RefreshClusterNodes),
            ]
            .spacing(8),
        );
        layout = layout.push(iced::widget::rule::horizontal(1));

        // summary from server status
        if let Some(status) = &self.server_status {
            let c = &status.cluster;
            if !c.enabled {
                layout = layout.push(text("Cluster not enabled on this server.").size(12));
                return layout.into();
            }

            layout = layout.push(text("Summary").size(14));
            layout = layout.push(
                row![
                    text("State").size(11),
                    text(format!("{}", c.state)).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Cluster ID").size(11),
                    text(&c.cluster_id).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Node ID").size(11),
                    text(&c.node_id).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Epoch").size(11),
                    text(format!("{}", c.epoch_id)).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Leader").size(11),
                    text(&c.leader_id).size(11),
                ]
                .spacing(8),
            );
            layout = layout.push(
                row![
                    text("Quorum").size(11),
                    text(format!(
                        "{}/{} voters reachable (need {})",
                        c.reachable_voters, c.voter_count, c.quorum
                    ))
                    .size(11),
                ]
                .spacing(8),
            );
            if let Some(reason) = &c.fenced_reason {
                layout = layout.push(
                    row![text("Fenced").size(11), text(reason).size(11),].spacing(8),
                );
            }
        }

        layout = layout.push(iced::widget::rule::horizontal(1));

        // node list
        match &self.cluster_nodes {
            None => {
                layout = layout.push(text("Loading nodes...").size(12));
            }
            Some(Err(e)) => {
                layout = layout.push(text(format!("Error: {}", e)).size(12));
            }
            Some(Ok(data)) => {
                layout = layout.push(text("Nodes").size(14));

                // header
                layout = layout.push(
                    row![
                        text("Node ID").size(10).width(180),
                        text("State").size(10).width(80),
                        text("Voter").size(10).width(50),
                        text("Reachable").size(10).width(70),
                        text("Disks").size(10).width(50),
                    ]
                    .spacing(4),
                );
                layout = layout.push(iced::widget::rule::horizontal(1));

                for node in &data.nodes {
                    let voter_label = if node.voter { "yes" } else { "no" };
                    let reach_label = if node.reachable { "yes" } else { "NO" };
                    layout = layout.push(
                        row![
                            text(&node.node_id).size(10).width(180),
                            text(format!("{}", node.state)).size(10).width(80),
                            text(voter_label).size(10).width(50),
                            text(reach_label).size(10).width(70),
                            text(format!("{}", node.total_disks)).size(10).width(50),
                        ]
                        .spacing(4),
                    );
                }

                // summary
                if !data.nodes.is_empty() {
                    layout = layout.push(iced::widget::rule::horizontal(1));
                    let reachable = data.nodes.iter().filter(|n| n.reachable).count();
                    let voters = data.nodes.iter().filter(|n| n.voter).count();
                    layout = layout.push(
                        text(format!(
                            "{} nodes ({} voters, {} reachable)",
                            data.nodes.len(),
                            voters,
                            reachable,
                        ))
                        .size(11),
                    );
                }
            }
        }

        layout.into()
    }
}
