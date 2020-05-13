use pest::Parser;
use pest_derive::Parser;

use anyhow::Error;

#[derive(Parser)]
#[grammar = "nsupdate.pest"]
struct NSUpdateParser;

#[derive(Debug)]
pub struct NSUpdateActionAdd {
    pub domain: String,
    pub ttl: usize,
    pub record_type: String,
    pub priority: Option<usize>,
    pub content: String,
}

#[derive(Debug)]
pub struct NSUpdateActionDelete {
    pub domain: String,
    pub record_type: String,
}

#[derive(Debug)]
pub enum NSUpdateAction {
    Add(NSUpdateActionAdd),
    Delete(NSUpdateActionDelete),
}

#[derive(Debug)]
pub enum NSUpdateCommand {
    Update(NSUpdateAction),
    Send,
}

#[derive(Debug, Default)]
pub struct NSUpdateQueue {
    inner: Vec<NSUpdateCommand>,
    send: bool,
}

impl NSUpdateQueue {
    pub async fn new() -> Self {
        Self {
            send: false,
            ..Default::default()
        }
    }

    async fn push(&mut self, command: NSUpdateCommand) {
        self.inner.push(command);
    }

    async fn set_send(&mut self) {
        self.send = true;
    }

    pub async fn has_send(&self) -> bool {
        self.send
    }

    pub fn into_inner(self) -> Vec<NSUpdateCommand> {
        self.inner
    }

    // Separated because I'm going to make some kind of REPL with it
    pub async fn parse_text(&mut self, input: &str) -> Result<Option<String>, Error> {
        let mut lines = input.split("\n").into_iter();
        while !self.has_send().await {
            if let Some(line) = lines.next() {
                self.parse_command(line).await?;
            } else {
                break;
            }
        }
        let remaining: String = lines.collect::<Vec<&str>>().join("\n");
        Ok(if remaining.len() > 0 {
            Some(remaining)
        } else {
            None
        })
    }

    pub async fn len(&self) -> usize {
        self.inner.len()
    }

    // These are surely garbage code, but it just werks.
    pub async fn parse_command(&mut self, input: &str) -> Result<(), Error> {
        let input_pairs = NSUpdateParser::parse(Rule::line, input)?;
        for command in input_pairs {
            match command.as_rule() {
                Rule::update => {
                    for action in command.into_inner() {
                        self.push(NSUpdateCommand::Update({
                            match action.as_rule() {
                                Rule::add => {
                                    let mut parameters = action.into_inner();
                                    let domain = parameters.next().unwrap().as_str().to_string();
                                    let ttl = parameters.next().unwrap().as_str().parse()?;
                                    let record_type = parameters
                                        .clone()
                                        .skip(1)
                                        .next()
                                        .unwrap()
                                        .as_str()
                                        .to_string();
                                    let (priority, content) = if parameters
                                        .clone()
                                        .skip(3)
                                        .next()
                                        .is_none()
                                    {
                                        (
                                            None,
                                            parameters.skip(2).next().unwrap().as_str().to_string(),
                                        )
                                    } else {
                                        (
                                            Some(
                                                parameters
                                                    .clone()
                                                    .skip(2)
                                                    .next()
                                                    .unwrap()
                                                    .as_str()
                                                    .parse()?,
                                            ),
                                            parameters.skip(3).next().unwrap().as_str().to_string(),
                                        )
                                    };
                                    NSUpdateAction::Add(NSUpdateActionAdd {
                                        domain,
                                        ttl,
                                        record_type,
                                        priority,
                                        content,
                                    })
                                }
                                Rule::delete => {
                                    let mut parameters = action.into_inner();
                                    NSUpdateAction::Delete(NSUpdateActionDelete {
                                        domain: parameters.next().unwrap().as_str().to_string(),
                                        record_type: parameters
                                            .next()
                                            .unwrap()
                                            .as_str()
                                            .to_string(),
                                    })
                                }
                                _ => unreachable!(),
                            }
                        }))
                        .await
                    }
                }
                Rule::send => {
                    self.push(NSUpdateCommand::Send).await;
                    self.set_send().await;
                }
                Rule::EOI | Rule::WHITESPACE | Rule::COMMENT => continue,
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}
