use std::net::{IpAddr, Ipv4Addr};

/// The root hints: names and IPv4 addresses of the thirteen root servers.
///
/// Every lookup has to start somewhere, and these addresses are the one thing
/// a resolver cannot look up. They are shipped with the software and change
/// only once every few years.
pub const ROOT_SERVERS: [(&str, Ipv4Addr); 13] = [
    ("a.root-servers.net.", Ipv4Addr::new(198, 41, 0, 4)),
    ("b.root-servers.net.", Ipv4Addr::new(170, 247, 170, 2)),
    ("c.root-servers.net.", Ipv4Addr::new(192, 33, 4, 12)),
    ("d.root-servers.net.", Ipv4Addr::new(199, 7, 91, 13)),
    ("e.root-servers.net.", Ipv4Addr::new(192, 203, 230, 10)),
    ("f.root-servers.net.", Ipv4Addr::new(192, 5, 5, 241)),
    ("g.root-servers.net.", Ipv4Addr::new(192, 112, 36, 4)),
    ("h.root-servers.net.", Ipv4Addr::new(198, 97, 190, 53)),
    ("i.root-servers.net.", Ipv4Addr::new(192, 36, 148, 17)),
    ("j.root-servers.net.", Ipv4Addr::new(192, 58, 128, 30)),
    ("k.root-servers.net.", Ipv4Addr::new(193, 0, 14, 129)),
    ("l.root-servers.net.", Ipv4Addr::new(199, 7, 83, 42)),
    ("m.root-servers.net.", Ipv4Addr::new(202, 12, 27, 33)),
];

pub fn root_addresses() -> Vec<IpAddr> {
    ROOT_SERVERS
        .iter()
        .map(|(_, address)| IpAddr::V4(*address))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn there_are_thirteen_distinct_root_servers() {
        let addresses: HashSet<IpAddr> = root_addresses().into_iter().collect();
        assert_eq!(addresses.len(), 13);
        let letters: String = ROOT_SERVERS.iter().map(|(name, _)| &name[..1]).collect();
        assert_eq!(letters, "abcdefghijklm");
    }
}
