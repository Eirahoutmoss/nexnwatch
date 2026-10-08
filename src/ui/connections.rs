//! Process bağlantıları — uzak IP/port, ters DNS, bağlantı başına bayt.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Instant;

use iced::widget::{column, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::app::{App, Message};
use crate::collectors::etw::{ConnBytes, ConnKey, Source};
use crate::collectors::rdns;
use crate::theme::p;
use crate::ui::{card, chip, th, tree};
use crate::units;

pub struct ConnRow {
    pub pid: u32,
    pub key: ConnKey,
    pub remote: SocketAddr,
    pub local: SocketAddr,
    pub bytes: ConnBytes,
}

/// Olaydaki iki uçtan hangisinin uzak olduğunu yerel IP listesine göre seç.
fn orient(app: &App, k: &ConnKey) -> (SocketAddr, SocketAddr) {
    let a_local = app.local_ips.contains(&k.a.ip()) || k.a.ip().is_loopback();
    let b_local = app.local_ips.contains(&k.b.ip()) || k.b.ip().is_loopback();
    match (a_local, b_local) {
        (true, false) => (k.b, k.a),
        _ => (k.a, k.b), // varsayılan: daddr uzak uç
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let now = Instant::now();

    // Kapsam: seçili/otomatik ağaçtaki PID'ler ya da tümü.
    let scope: Option<(HashSet<u32>, String)> = if app.conn_scope_all {
        None
    } else {
        let children = tree::children_map(app);
        tree::build(app, &children).map(|t| {
            let name = t.rows.first().map(|r| r.name.clone()).unwrap_or_default();
            (t.rows.iter().map(|r| r.pid).collect(), name)
        })
    };

    let q = app.conn_filter.to_lowercase();
    let mut rows: Vec<ConnRow> = app
        .etw
        .connections()
        .into_iter()
        .filter(|(k, _)| scope.as_ref().is_none_or(|(set, _)| set.contains(&k.pid)))
        .map(|(key, bytes)| {
            let (remote, local) = orient(app, &key);
            ConnRow { pid: key.pid, key, remote, local, bytes }
        })
        .filter(|r| {
            if q.is_empty() {
                return true;
            }
            let name = app.process(r.pid).map(|p| p.name.to_lowercase()).unwrap_or_default();
            let host = app.resolver.lookup(r.remote.ip()).unwrap_or_default().to_lowercase();
            name.contains(&q) || r.remote.to_string().contains(&q) || host.contains(&q)
        })
        .collect();
    rows.sort_by(|a, b| (b.bytes.rx + b.bytes.tx).cmp(&(a.bytes.rx + a.bytes.tx)));
    let total = rows.len();

    let w = |v: f32| Length::Fixed(v);
    let header = row![
        th("PROCESS", w(150.0)),
        th("PROTO", w(46.0)),
        th("UZAK UÇ", w(230.0)),
        th("ANA BİLGİSAYAR / SERVİS", Length::Fill),
        th("YEREL", w(64.0)),
        th("↓ ALINAN", w(78.0)),
        th("↑ GÖNDERİLEN", w(86.0)),
        th("SON", w(64.0)),
    ]
    .spacing(6);

    let mut list = column![].spacing(2);
    for r in rows.iter().take(200) {
        let name = app.process(r.pid).map(|p| p.name.clone()).unwrap_or_else(|| format!("PID {}", r.pid));
        let host = app.resolver.lookup(r.remote.ip());
        let svc = rdns::service(r.remote.port()).or_else(|| rdns::service(r.local.port()));
        let host_text = match (host, svc) {
            (Some(h), Some(s)) => format!("{h} · {s}"),
            (Some(h), None) => h,
            (None, Some(s)) => s.to_string(),
            (None, None) => "—".into(),
        };
        let ago = now.duration_since(r.bytes.last_seen).as_secs();
        let fresh = ago < 5;
        list = list.push(
            row![
                text(format!("{name} ({})", r.pid)).size(11).color(p().text).width(w(150.0)),
                text(r.key.proto.label()).size(11).color(p().muted).width(w(46.0)),
                text(r.remote.to_string()).size(11).color(if fresh { p().title } else { p().text }).width(w(230.0)),
                text(host_text).size(11).color(p().muted).width(Length::Fill),
                text(r.local.port().to_string()).size(11).color(p().muted).width(w(64.0)),
                text(units::bytes(r.bytes.rx)).size(11).color(p().rx).width(w(78.0)),
                text(units::bytes(r.bytes.tx)).size(11).color(p().tx).width(w(86.0)),
                text(if fresh { "şimdi".to_string() } else { units::ago(ago) })
                    .size(11)
                    .color(if fresh { p().good } else { p().muted })
                    .width(w(64.0)),
            ]
            .spacing(6),
        );
    }

    let scope_label = match &scope {
        Some((_, name)) if !name.is_empty() => format!("Ağaç: {name}"),
        Some(_) => "Ağaç".into(),
        None => "Tüm process'ler".into(),
    };
    let controls = row![
        text_input("Process, IP veya ana bilgisayar ara…", &app.conn_filter)
            .on_input(Message::ConnFilter)
            .size(12)
            .padding(7)
            .width(Length::Fixed(280.0)),
        chip(scope_label, !app.conn_scope_all, Message::ConnScopeAll(false)),
        chip("Tümü".into(), app.conn_scope_all, Message::ConnScopeAll(true)),
        Space::new().width(Length::Fill),
        text(format!("{total} bağlantı")).size(11).color(p().muted),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let note = match app.etw.source() {
        Source::Etw => "ETW olaylarından · TCP + UDP · son 15 dk içinde etkin olanlar",
        Source::IpHelper => "IP Helper yedeği · yalnızca kurulu TCP bağlantıları",
    };

    let body: Element<'_, Message> = if total == 0 {
        text("Gösterilecek bağlantı yok.").size(11).color(p().muted).into()
    } else {
        scrollable(list).height(Length::Fixed(320.0)).into()
    };

    card("Bağlantılar", note, column![controls, header, body].spacing(8))
}
