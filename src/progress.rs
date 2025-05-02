use std::collections::HashMap;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct Progress {
    container: MultiProgress,
    panels: HashMap<String, ProgressBar>,
}

pub enum PanelType {
    Bar(String, u64),
    Message(String),
}

pub enum Update {
    #[allow(unused)]
    Position(u64, Option<String>),
    Incr(Option<String>),
}

impl Progress {
    pub fn new(panel_defs: Vec<PanelType>) -> Self {
        let bar_style = ProgressStyle::with_template(
            "{spinner:.white} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>5}/{len:5}",
        )
        .unwrap()
        .progress_chars("#>-");

        let message_style = ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

        let container = MultiProgress::new();
        let mut panels = HashMap::new();

        for def in panel_defs.into_iter() {
            match def {
                PanelType::Bar(key, len) => {
                    let pb = container.add(ProgressBar::new(len));
                    pb.set_style(bar_style.clone());
                    panels.insert(key, pb);
                }
                PanelType::Message(key) => {
                    let pb = container.add(ProgressBar::new(8));
                    pb.set_style(message_style.clone());
                    panels.insert(key, pb);
                }
            }
        }

        Progress { container, panels }
    }

    pub fn update(&self, key: &str, u: Update) {
        if let Some(pb) = self.panels.get(key) {
            match u {
                Update::Position(pos, msg) => {
                    pb.set_position(pos);
                    if let Some(msg) = msg {
                        pb.set_message(msg);
                    }
                }
                Update::Incr(msg) => {
                    pb.inc(1);
                    if let Some(msg) = msg {
                        pb.set_message(msg);
                    }
                }
            }
        }
    }

    #[allow(unused)]
    pub fn finish(&self, key: &str) {
        if let Some(pb) = self.panels.get(key) {
            pb.finish();
        }
    }

    pub fn finish_all(&self) {
        for (_, pb) in self.panels.iter() {
            pb.finish();
        }
    }

    #[allow(unused)]
    pub fn finish_with_message(&self, key: &str, message: &'static str) {
        if let Some(pb) = self.panels.get(key) {
            pb.finish_with_message(message);
        }
    }

    pub fn clear(&self) {
        self.container.clear().unwrap_or(())
    }

    pub fn println<I: AsRef<str>>(&self, msg: I) {
        self.container.println(msg).unwrap_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn progress_bar() {
        let panels = vec![
            PanelType::Bar(String::from("bar"), 100),
            PanelType::Message(String::from("msg")),
        ];

        let p = Progress::new(panels);

        for i in 0..100 {
            p.update("bar", Update::Incr(None));
            if i % 5 == 0 {
                p.update(
                    "msg",
                    Update::Incr(Some(format!("{} is progressing...", i))),
                );
            }
            thread::sleep(Duration::from_millis(15));
        }

        p.finish("bar");
        p.finish("msg");
        p.println("Done");
        p.clear();
    }
}
