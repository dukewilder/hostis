//! The keeper's side of a station's page: writing the programme, shows and notices.
//! What the page shows is worked out in pyrate_core::page, shared with the browser.
//!
//! No rights reserved.

use anyhow::{Context, Result, anyhow};
use pyrate_core::page::{page_of, programme_from, programme_json};
use pyrate_core::{Body, Hash, StationId};
use serde_json::{Value, json};

use crate::{Inner, describe_track, hex, now_ms, unhex};

impl Inner {
    fn write_body(&self, st: StationId, body: Body) -> Result<Hash> {
        let e = self.store.lock().unwrap().write(st, now_ms(), body)?;
        Ok(e.hash()?)
    }

    pub(crate) fn page_call(&self, cmd: &str, st: StationId, req: &Value) -> Result<Value> {
        match cmd {
            "page" => {
                let s = self.store.lock().unwrap();
                let a = s.logs.get(&st).context("this station's log has not arrived yet")?;
                let mut v = page_of(a, now_ms(), req["days"].as_u64().unwrap_or(7).min(28)).map_err(|e| anyhow!("{e}"))?;
                v["own"] = json!(s.keys.contains_key(&st));
                Ok(v)
            }
            "programme" => {
                let s = self.store.lock().unwrap();
                let a = s.logs.get(&st).context("no log")?;
                let lib: Vec<Value> = a.library()?.into_iter().map(|(h, e)| {
                    let by = match &e.entry.body { Body::Deputed { by, .. } => hex(by), _ => st.to_hex() };
                    json!({ "track": hex(&h), "title": describe_track(a, &h).0, "added_ms": e.entry.at_ms, "added_by": by })
                }).collect();
                let hands: Vec<Value> = a.deputies().iter().map(|k| json!({ "key": hex(k), "label": a.entries().iter().find_map(|e| match e.entry.body() { Body::Deputy { key, label } if key == k => Some(label.clone()), _ => None }) })).collect();
                Ok(json!({ "programme": a.programme().as_ref().map(programme_json), "library": lib, "hands": hands }))
            }
            "programme_set" => {
                let p = programme_from(&req["programme"]).map_err(|e| anyhow!("{e}"))?;
                Ok(json!({ "entry": hex(&self.write_body(st, Body::Programme(p))?) }))
            }
            "show_add" => {
                let at_ms = req["at_ms"].as_u64().context("at_ms")?;
                let tracks: Vec<Hash> = req["tracks"].as_array().into_iter().flatten().filter_map(|h| h.as_str().and_then(|h| unhex(h).ok())).take(200).collect();
                let live_min = req["live_min"].as_u64().unwrap_or(0).min(24 * 60) as u32;
                if tracks.is_empty() && live_min == 0 {
                    anyhow::bail!("a show needs recordings or a live hour");
                }
                let body = Body::Show {
                    title: req["title"].as_str().unwrap_or("").chars().take(80).collect(),
                    about: req["about"].as_str().unwrap_or("").chars().take(400).collect(),
                    at_ms,
                    tracks,
                    live_min,
                };
                Ok(json!({ "show": hex(&self.write_body(st, body)?) }))
            }
            "show_cancel" => {
                let show = unhex(req["show"].as_str().context("show")?)?;
                self.write_body(st, Body::Unshow { show })?;
                Ok(json!({ "ok": true }))
            }
            "notice_post" => {
                let text: String = req["text"].as_str().context("text")?.trim().chars().take(600).collect();
                if text.is_empty() {
                    anyhow::bail!("a notice needs words");
                }
                Ok(json!({ "notice": hex(&self.write_body(st, Body::Notice { text, until_ms: req["until_ms"].as_u64().unwrap_or(0) })?) }))
            }
            "notice_remove" => {
                let notice = unhex(req["notice"].as_str().context("notice")?)?;
                self.write_body(st, Body::Unnotice { notice })?;
                Ok(json!({ "ok": true }))
            }
            _ => Err(anyhow!("unknown command {cmd}")),
        }
    }
}
