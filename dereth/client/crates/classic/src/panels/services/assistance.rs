use super::*;
const NAME_PROMPT: &str = "Enter exact name of offending character here.";
const ABUSE_PROMPT: &str =
    "Enter a brief description of the offensive behavior, then click the 'Continue' button below.";
const URGENT_PROMPT: &str =
    "Enter here your request for urgent assistance, then click the 'Continue' button below.";
pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    let urgent = id == "urgent-assistance";
    Some(Box::new(Assistance {
        urgent,
        stage: 0,
        name: NAME_PROMPT.into(),
        text: if urgent { URGENT_PROMPT } else { ABUSE_PROMPT }.into(),
        can_continue: false,
    }))
}
#[derive(Debug)]
struct Assistance {
    urgent: bool,
    stage: u8,
    name: String,
    text: String,
    can_continue: bool,
}
fn inserted_nonblank(old: &str, new: &str) -> bool {
    let old: Vec<_> = old.chars().collect();
    let new: Vec<_> = new.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    new[prefix..new.len() - suffix]
        .iter()
        .any(|c| *c != ' ' && *c != '\n')
}
impl Panel for Assistance {
    fn id(&self) -> &'static str {
        if self.urgent {
            "urgent-assistance"
        } else {
            "abuse"
        }
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = tiled(300, crate::panels::side_height() - 25, "0600128A");
        match self.stage {
            0 => {
                let intro = if self.urgent {
                    "Use this option ONLY if you are stuck in a situation which prevents you from playing the game.\n\nTo file a report of a less urgent nature, such as a general question, suggestion, or observation, please contact Customer Service via this link:\n\nhttp://ac.turbinegames.com/index.php?page_id=177"
                } else {
                    "You should only click the 'Report Abuse' button to report behavior which is threatening or offensive.\n\nMisuse of this function violates the Code of Conduct.\n\nIf you have a less urgent or milder complaint, please contact Customer Service via this link:\n\nhttp://ac.turbinegames.com/index.php?page_id=177"
                };
                label(
                    &mut f,
                    rect(5, 40, 290, if self.urgent { 240 } else { 200 }),
                    intro,
                    "15-6",
                );
                if self.urgent {
                    f.button("begin", rect(30, 285, 120, 36), "OK", true);
                    f.button("cancel", rect(160, 285, 120, 36), "Cancel", true);
                } else {
                    f.button("begin", rect(30, 240, 240, 36), "Report Abuse", true);
                    f.button("cancel", rect(30, 285, 240, 36), "Cancel", true);
                }
            }
            1 => {
                f.image("060012C5", rect(4, 270, 292, 8), true, false);
                if self.urgent {
                    f.image("0600147B", rect(0, 40, 300, 82), false, false);
                    centered(
                        &mut f,
                        rect(5, 20, 290, 20),
                        "Describe the Situation",
                        "16-7",
                    );
                    f.edit(
                        "text",
                        rect(8, 40, 284, 82),
                        &self.text,
                        usize::MAX,
                        true,
                        true,
                    );
                    label(&mut f,rect(8,142,284,128),"Envoys will help you as soon as they can.  Calls are answered in order of urgency.  If you require assistance please do not log off the character which submitted the request.  Please be as descriptive as possible.  Please submit one call per issue, and please wait 30 minutes before resubmitting the request.  Abuse of this feature can result in your being removed from the game.","15-6");
                } else {
                    f.image("0600147C", rect(0, 42, 300, 20), false, false);
                    f.image("0600147B", rect(0, 120, 300, 82), false, false);
                    centered(&mut f, rect(5, 22, 290, 20), "Offending Character:", "16-7");
                    centered(
                        &mut f,
                        rect(5, 100, 290, 20),
                        "Description of Offense:",
                        "16-7",
                    );
                    f.edit(
                        "name",
                        rect(8, 42, 284, 20),
                        &self.name,
                        usize::MAX,
                        false,
                        true,
                    );
                    f.edit(
                        "text",
                        rect(8, 120, 284, 82),
                        &self.text,
                        usize::MAX,
                        true,
                        true,
                    );
                }
                f.button(
                    "send",
                    rect(25, 290, 120, 36),
                    "Continue",
                    self.can_continue,
                );
                f.button("cancel", rect(150, 290, 120, 36), "Cancel", true);
            }
            _ => {
                f.image("060012C5", rect(4, 270, 292, 8), true, false);
                centered(
                    &mut f,
                    rect(5, 105, 290, 120),
                    if self.urgent {
                        "Your message has been sent.\nAn Envoy may be contacting you shortly. However, a response cannot always be guaranteed."
                    } else {
                        c.classic
                            .abuse_response
                            .as_deref()
                            .unwrap_or("Registerting your complaint ...")
                    },
                    "16-7",
                );
                f.button("done", rect(25, 290, 240, 36), "Done", true);
            }
        }
        f = translated(f, 25, crate::panels::side_height());
        f.image("06001477", rect(0, 0, 276, 30), false, false);
        centered(
            &mut f,
            rect(0, 2, 276, 20),
            if self.urgent {
                "Urgent Assistance"
            } else {
                "Report Abuse"
            },
            "16-7",
        );
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x06001283, 0x06001282, 0x06001283],
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Edit { id, text } if self.stage == 1 => {
                if id == "text" {
                    self.can_continue |= inserted_nonblank(&self.text, &text);
                    self.text = text;
                } else if id == "name" {
                    self.name = text;
                }
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "close" | "cancel" | "done" => vec![PanelAction::Close],
                "begin" if self.stage == 0 => {
                    self.stage = 1;
                    vec![]
                }
                "send" if self.stage == 1 && self.can_continue => {
                    if !self.urgent {
                        let name = self.name.trim_start_matches(' ');
                        if name.is_empty()
                            || name
                                .to_ascii_lowercase()
                                .starts_with(&NAME_PROMPT.to_ascii_lowercase())
                        {
                            return vec![PanelAction::Host(HostAction::LocalFeedback {
                                severity: if self.name.is_empty() {
                                    crate::panels::FeedbackSeverity::Information
                                } else {
                                    crate::panels::FeedbackSeverity::Warning
                                },
                                text: "Please specify the character to log.".into(),
                            })];
                        }
                        if name.to_ascii_lowercase().starts_with("off,") {
                            let Some((_, target)) =
                                name.split_once(' ').filter(|(_, t)| !t.is_empty())
                            else {
                                return vec![PanelAction::Host(HostAction::LocalFeedback {
                                    severity: crate::panels::FeedbackSeverity::Warning,
                                    text: "Please specify the character to log.".into(),
                                })];
                            };
                            let target = target.to_owned();
                            self.stage = 2;
                            self.can_continue = false;
                            return vec![PanelAction::Host(HostAction::AbuseLog {
                                target,
                                enabled: false,
                                complaint: self.text.clone(),
                            })];
                        }
                    }
                    self.stage = 2;
                    self.can_continue = false;
                    request(if self.urgent {
                        UiRequest::ChannelBroadcast {
                            channel: 0x400,
                            text: self.text.clone(),
                        }
                    } else {
                        UiRequest::AbuseLog {
                            target: self.name.trim_start_matches(' ').into(),
                            complaint: self.text.clone(),
                        }
                    })
                }
                _ => vec![],
            },
            _ => vec![],
        }
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn continue_latches_on_inserted_nonspace_and_not_deletion() {
        assert!(!inserted_nonblank("abc", "ab"));
        assert!(!inserted_nonblank("", " \n"));
        assert!(inserted_nonblank("abc", "abcd"));
    }
}
