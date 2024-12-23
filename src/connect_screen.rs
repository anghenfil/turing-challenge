use eframe::egui::{Context, Pos2, Rect, Vec2};
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use crate::{ApplicationState, InterTaskMessageToNetworkTask};

pub fn render_connect_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
    egui::CentralPanel::default().show(ctx, |ui| {
        let total_width = ui.available_width();
        let content_width = total_width*0.6;
        let side_width = (total_width - content_width)/2.0;

        StripBuilder::new(ui)
            .size(Size::exact(side_width))
            .size(Size::exact(content_width))
            .size(Size::exact(side_width))
            .horizontal(|mut strip|{
               strip.cell(|ui|{

               });
               strip.cell(|ui|{
                   ui.vertical_centered(|ui|{
                       ui.add_space(10.0);
                       ui.heading("Connect to another player");
                       ui.label(format!("Currently listening on {}:{}.", app.settings.bind_to_host, app.settings.port));
                       ui.add_space(40.0);

                       if let Some(warning) = &app.warning{
                           ui.label(warning).highlight();
                           ui.add_space(10.0);
                       }

                       ui.text_edit_singleline(&mut app.connect_host);
                       ui.add_space(10.0);
                       if ui.button("Connect").clicked(){
                           app.mpsc_sender.try_send(InterTaskMessageToNetworkTask::ConnectTo{ host_string: app.connect_host.clone() }).unwrap();
                       }
                   });
               });
               strip.cell(|ui|{

               });
            });
    });
}