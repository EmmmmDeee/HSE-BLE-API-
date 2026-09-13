//! A virtual BLE advertiser on netsimd's HCI socket.
//!
//! The emulator's radios are simulated by netsimd (rootcanal inside it): the
//! guest's Bluetooth controller is one device on its medium, and every TCP
//! connection to the daemon's HCI port (`--hci-port`, 6402 by default) is
//! another — a fresh virtual controller that lives as long as the
//! connection — driven over H4, the UART transport framing (a packet-type
//! byte, then the HCI packet). Configured as a legacy advertiser with a
//! static random address and the device name in its advertising data, that
//! controller is what the guest's scan reports. Dependency-free like the
//! rest of xtask: the six commands the advertiser needs (Core Specification
//! vol. 4 part E) are encoded and decoded by hand.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// H4 packet types.
const H4_COMMAND: u8 = 0x01;
const H4_ACL: u8 = 0x02;
const H4_SCO: u8 = 0x03;
const H4_EVENT: u8 = 0x04;
const H4_ISO: u8 = 0x05;

/// Event codes.
const COMMAND_COMPLETE: u8 = 0x0E;
const COMMAND_STATUS: u8 = 0x0F;

/// Opcodes (`OGF << 10 | OCF`).
pub const RESET: u16 = 0x0C03;
pub const READ_BD_ADDR: u16 = 0x1009;
pub const LE_SET_RANDOM_ADDRESS: u16 = 0x2005;
pub const LE_SET_ADVERTISING_PARAMETERS: u16 = 0x2006;
pub const LE_SET_ADVERTISING_DATA: u16 = 0x2008;
pub const LE_SET_ADVERTISING_ENABLE: u16 = 0x200A;

/// The advertising data field is always sent whole.
const ADVERTISING_DATA_LEN: usize = 31;

// ===== packets (pure, unit-tested) =====

/// The H4 packet of one command.
pub fn command_packet(opcode: u16, parameters: &[u8]) -> Result<Vec<u8>, String> {
    let length = u8::try_from(parameters.len())
        .map_err(|_| format!("{} parameter bytes do not fit a command", parameters.len()))?;
    let mut packet = Vec::with_capacity(4 + parameters.len());
    packet.push(H4_COMMAND);
    packet.extend_from_slice(&opcode.to_le_bytes());
    packet.push(length);
    packet.extend_from_slice(parameters);
    Ok(packet)
}

/// One H4 packet from `reader`: its type and the HCI packet after the type
/// byte (header and payload, as the spec lays them out).
pub fn read_packet(reader: &mut impl Read) -> Result<(u8, Vec<u8>), String> {
    let mut kind = [0u8; 1];
    reader
        .read_exact(&mut kind)
        .map_err(|e| format!("reading the packet type: {e}"))?;
    // Header length, and where the payload length sits in it (one byte, or
    // two little-endian bytes for ACL and ISO data).
    let (header_len, length_at, two_bytes) = match kind[0] {
        H4_COMMAND => (3, 2, false),
        H4_ACL => (4, 2, true),
        H4_SCO => (3, 2, false),
        H4_EVENT => (2, 1, false),
        H4_ISO => (4, 2, true),
        other => return Err(format!("unknown H4 packet type 0x{other:02X}")),
    };
    let mut packet = vec![0u8; header_len];
    reader
        .read_exact(&mut packet)
        .map_err(|e| format!("reading the packet header: {e}"))?;
    let payload_len = if two_bytes {
        usize::from(u16::from_le_bytes([
            packet[length_at],
            packet[length_at + 1],
        ]))
    } else {
        usize::from(packet[length_at])
    };
    packet.resize(header_len + payload_len, 0);
    reader
        .read_exact(&mut packet[header_len..])
        .map_err(|e| format!("reading the packet payload: {e}"))?;
    Ok((kind[0], packet))
}

/// What an event says about a command.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// A Command Complete: the return parameters after the status.
    Complete {
        /// The command's opcode.
        opcode: u16,
        /// The status (0 is success).
        status: u8,
        /// The return parameters after the status byte.
        returned: &'a [u8],
    },
    /// A Command Status: the command was accepted (status 0) or refused.
    Status {
        /// The command's opcode.
        opcode: u16,
        /// The status (0 means the command proceeds asynchronously).
        status: u8,
    },
}

/// The outcome an event packet (code, length, parameters) carries, if it is
/// a Command Complete or a Command Status event.
pub fn command_outcome(event: &[u8]) -> Option<Outcome<'_>> {
    let (&code, rest) = event.split_first()?;
    let (&length, parameters) = rest.split_first()?;
    let parameters = parameters.get(..usize::from(length))?;
    match code {
        COMMAND_COMPLETE => {
            // Num_HCI_Command_Packets, Command_Opcode, Return_Parameters
            // (whose first byte is the status).
            let opcode = u16::from_le_bytes([*parameters.get(1)?, *parameters.get(2)?]);
            let status = *parameters.get(3)?;
            Some(Outcome::Complete {
                opcode,
                status,
                returned: &parameters[4..],
            })
        }
        COMMAND_STATUS => {
            // Status, Num_HCI_Command_Packets, Command_Opcode.
            let status = *parameters.first()?;
            let opcode = u16::from_le_bytes([*parameters.get(2)?, *parameters.get(3)?]);
            Some(Outcome::Status { opcode, status })
        }
        _ => None,
    }
}

/// The names of the statuses a misdriven advertiser can get.
pub fn status_name(status: u8) -> &'static str {
    match status {
        0x00 => "Success",
        0x01 => "Unknown HCI Command",
        0x0C => "Command Disallowed",
        0x11 => "Unsupported Feature or Parameter Value",
        0x12 => "Invalid HCI Command Parameters",
        _ => "another error code",
    }
}

/// `AA:BB:CC:DD:EE:FF` as bytes in the written order (most significant
/// first).
pub fn parse_address(text: &str) -> Result<[u8; 6], String> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() != 6 {
        return Err(format!("`{text}` is not a six-octet address"));
    }
    let mut address = [0u8; 6];
    for (slot, part) in address.iter_mut().zip(parts) {
        if part.len() != 2 {
            return Err(format!("`{text}` is not a six-octet address"));
        }
        *slot = u8::from_str_radix(part, 16)
            .map_err(|e| format!("`{text}` is not a hexadecimal address: {e}"))?;
    }
    Ok(address)
}

/// The wire form (least significant byte first) of a written address.
pub fn wire_address(text: &str) -> Result<[u8; 6], String> {
    let mut address = parse_address(text)?;
    address.reverse();
    Ok(address)
}

/// The written form of an address the wire carries least significant byte
/// first.
pub fn format_wire_address(wire: &[u8]) -> String {
    wire.iter()
        .rev()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Whether a written address is a static random one (its two top bits set),
/// the kind a controller may advertise from without an identity.
pub fn is_static_random(address: &[u8; 6]) -> bool {
    address[0] & 0xC0 == 0xC0
}

/// `LE_Set_Advertising_Parameters`: connectable undirected advertising
/// (`ADV_IND`) at one fixed `interval` (0.625 ms units) from the
/// controller's random address, on all three channels, for any scanner.
pub fn advertising_parameters(interval: u16) -> Vec<u8> {
    let mut parameters = Vec::with_capacity(15);
    parameters.extend_from_slice(&interval.to_le_bytes()); // Advertising_Interval_Min
    parameters.extend_from_slice(&interval.to_le_bytes()); // Advertising_Interval_Max
    parameters.push(0x00); // Advertising_Type: ADV_IND
    parameters.push(0x01); // Own_Address_Type: random
    parameters.push(0x00); // Peer_Address_Type
    parameters.extend_from_slice(&[0u8; 6]); // Peer_Address
    parameters.push(0x07); // Advertising_Channel_Map: 37, 38 and 39
    parameters.push(0x00); // Advertising_Filter_Policy: any scanner
    parameters
}

/// `LE_Set_Advertising_Data`: the flags (LE General Discoverable, BR/EDR
/// not supported) and the complete local `name`, in the whole 31-byte field
/// behind its significant length.
pub fn advertising_data(name: &str) -> Result<Vec<u8>, String> {
    let name = name.as_bytes();
    let significant = 3 + 2 + name.len();
    if significant > ADVERTISING_DATA_LEN {
        return Err(format!(
            "the name is {} bytes; the advertising data has room for {}",
            name.len(),
            ADVERTISING_DATA_LEN - 5
        ));
    }
    let mut parameters = Vec::with_capacity(1 + ADVERTISING_DATA_LEN);
    parameters.push(significant as u8); // fits: at most 31
    parameters.extend_from_slice(&[0x02, 0x01, 0x06]); // Flags
    parameters.push(name.len() as u8 + 1); // fits: at most 27
    parameters.push(0x09); // Complete Local Name
    parameters.extend_from_slice(name);
    parameters.resize(1 + ADVERTISING_DATA_LEN, 0);
    Ok(parameters)
}

// ===== the controller =====

/// One virtual controller: a connection to netsimd's HCI socket, which the
/// daemon removes from its model when the connection ends (on drop).
#[derive(Debug)]
pub struct Controller {
    stream: TcpStream,
    timeout: Duration,
}

impl Controller {
    /// Connects to the HCI socket on the loopback `port`.
    pub fn connect(port: u16, timeout: Duration) -> Result<Self, String> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|e| format!("connecting to 127.0.0.1:{port}: {e}"))?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("setting the read timeout: {e}"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| format!("setting the write timeout: {e}"))?;
        Ok(Self { stream, timeout })
    }

    /// Sends one command and waits for its Command Complete with status 0:
    /// the return parameters after the status. Events about anything else
    /// are skipped; a refusal names its status; a Command Status (the
    /// command proceeding asynchronously) is not what these commands answer.
    pub fn command(
        &mut self,
        name: &str,
        opcode: u16,
        parameters: &[u8],
    ) -> Result<Vec<u8>, String> {
        let packet = command_packet(opcode, parameters).map_err(|e| format!("{name}: {e}"))?;
        self.stream
            .write_all(&packet)
            .map_err(|e| format!("{name}: sending the command: {e}"))?;
        let deadline = Instant::now() + self.timeout;
        while Instant::now() <= deadline {
            let (kind, packet) =
                read_packet(&mut self.stream).map_err(|e| format!("{name}: {e}"))?;
            if kind != H4_EVENT {
                continue;
            }
            match command_outcome(&packet) {
                Some(Outcome::Complete {
                    opcode: answered,
                    status,
                    returned,
                }) if answered == opcode => {
                    if status != 0 {
                        return Err(format!(
                            "{name}: status 0x{status:02X} ({})",
                            status_name(status)
                        ));
                    }
                    return Ok(returned.to_vec());
                }
                Some(Outcome::Status {
                    opcode: answered,
                    status,
                }) if answered == opcode => {
                    return Err(format!(
                        "{name}: a Command Status (0x{status:02X}, {}) where a Command Complete was expected",
                        status_name(status)
                    ));
                }
                _ => {}
            }
        }
        Err(format!(
            "{name}: no Command Complete within {}s",
            self.timeout.as_secs()
        ))
    }

    /// `HCI_Reset`: the controller in its initial state.
    pub fn reset(&mut self) -> Result<(), String> {
        self.command("HCI_Reset", RESET, &[]).map(drop)
    }

    /// `Read_BD_ADDR`: the controller's public address, written out.
    pub fn read_bd_addr(&mut self) -> Result<String, String> {
        let returned = self.command("Read_BD_ADDR", READ_BD_ADDR, &[])?;
        if returned.len() != 6 {
            return Err(format!(
                "Read_BD_ADDR returned {} bytes, not an address",
                returned.len()
            ));
        }
        Ok(format_wire_address(&returned))
    }

    /// Starts legacy advertising from the static random `address` (what a
    /// scanner reports as the device's address) with `name` in the data,
    /// one PDU every `interval` × 0.625 ms.
    pub fn advertise(&mut self, address: &str, name: &str, interval: u16) -> Result<(), String> {
        if !is_static_random(&parse_address(address)?) {
            return Err(format!(
                "{address} is not a static random address (its two top bits must be set)"
            ));
        }
        self.command(
            "LE_Set_Random_Address",
            LE_SET_RANDOM_ADDRESS,
            &wire_address(address)?,
        )?;
        self.command(
            "LE_Set_Advertising_Parameters",
            LE_SET_ADVERTISING_PARAMETERS,
            &advertising_parameters(interval),
        )?;
        self.command(
            "LE_Set_Advertising_Data",
            LE_SET_ADVERTISING_DATA,
            &advertising_data(name)?,
        )?;
        self.command(
            "LE_Set_Advertising_Enable",
            LE_SET_ADVERTISING_ENABLE,
            &[0x01],
        )?;
        Ok(())
    }

    /// Ends the advertising; the controller itself goes with the connection.
    pub fn stop_advertising(&mut self) -> Result<(), String> {
        self.command(
            "LE_Set_Advertising_Enable",
            LE_SET_ADVERTISING_ENABLE,
            &[0x00],
        )
        .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;
    use std::thread;

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// What the fake controller saw: `(opcode, parameters)` per command.
    type Seen = Vec<(u16, Vec<u8>)>;

    /// A controller on a loopback port that records every command and
    /// answers each with a Command Complete of `status` (`Read_BD_ADDR`
    /// with an address), after sending `before_first_answer` once.
    fn fake_controller(
        status: u8,
        before_first_answer: Vec<u8>,
    ) -> (u16, thread::JoinHandle<Seen>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut prelude = Some(before_first_answer);
            while let Ok((H4_COMMAND, packet)) = read_packet(&mut stream) {
                let opcode = u16::from_le_bytes([packet[0], packet[1]]);
                seen.push((opcode, packet[3..].to_vec()));
                if let Some(prelude) = prelude.take() {
                    stream.write_all(&prelude).unwrap();
                }
                let returned: &[u8] = if opcode == READ_BD_ADDR {
                    &[0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
                } else {
                    &[]
                };
                let mut event = vec![H4_EVENT, COMMAND_COMPLETE, 4 + returned.len() as u8, 1];
                event.extend_from_slice(&opcode.to_le_bytes());
                event.push(status);
                event.extend_from_slice(returned);
                stream.write_all(&event).unwrap();
            }
            seen
        });
        (port, handle)
    }

    #[test]
    fn packets_are_encoded_and_read() {
        assert_eq!(
            command_packet(RESET, &[]).unwrap(),
            vec![0x01, 0x03, 0x0C, 0x00]
        );
        assert_eq!(
            command_packet(LE_SET_ADVERTISING_ENABLE, &[1]).unwrap(),
            vec![0x01, 0x0A, 0x20, 0x01, 0x01]
        );
        assert!(command_packet(RESET, &[0; 256]).is_err());

        // An ACL packet (two-byte length) then an event, back to back.
        let stream = [
            0x02, 0x40, 0x00, 0x03, 0x00, 0xAA, 0xBB, 0xCC, // ACL: handle 0x40, 3 bytes
            0x04, 0x0E, 0x04, 0x01, 0x03, 0x0C, 0x00, // Command Complete for Reset
        ];
        let mut cursor = Cursor::new(&stream[..]);
        assert_eq!(
            read_packet(&mut cursor).unwrap(),
            (H4_ACL, vec![0x40, 0x00, 0x03, 0x00, 0xAA, 0xBB, 0xCC])
        );
        let (kind, event) = read_packet(&mut cursor).unwrap();
        assert_eq!(kind, H4_EVENT);
        assert_eq!(
            command_outcome(&event),
            Some(Outcome::Complete {
                opcode: RESET,
                status: 0,
                returned: &[]
            })
        );
        assert!(read_packet(&mut cursor).is_err(), "the stream is spent");
        assert!(read_packet(&mut Cursor::new(&[0x50u8][..])).is_err());

        // A Command Status, an unrelated event, a truncated one.
        assert_eq!(
            command_outcome(&[0x0F, 0x04, 0x01, 0x01, 0x09, 0x10]),
            Some(Outcome::Status {
                opcode: READ_BD_ADDR,
                status: 0x01
            })
        );
        assert_eq!(command_outcome(&[0x3E, 0x03, 0x02, 0x01, 0x00]), None);
        assert_eq!(command_outcome(&[0x0E, 0x04, 0x01]), None);
        assert_eq!(status_name(0x0C), "Command Disallowed");
    }

    #[test]
    fn addresses_and_advertising_fields_are_encoded() {
        assert_eq!(
            parse_address("C0:DE:BE:AC:0D:01").unwrap(),
            [0xC0, 0xDE, 0xBE, 0xAC, 0x0D, 0x01]
        );
        assert_eq!(
            wire_address("C0:DE:BE:AC:0D:01").unwrap(),
            [0x01, 0x0D, 0xAC, 0xBE, 0xDE, 0xC0]
        );
        assert_eq!(
            format_wire_address(&[0x01, 0x0D, 0xAC, 0xBE, 0xDE, 0xC0]),
            "C0:DE:BE:AC:0D:01"
        );
        assert!(parse_address("C0:DE:BE:AC:0D").is_err());
        assert!(parse_address("C0:DE:BE:AC:0D:0G").is_err());
        assert!(parse_address("C0:DE:BE:AC:0D:001").is_err());
        assert!(is_static_random(&[0xC0, 0, 0, 0, 0, 1]));
        assert!(!is_static_random(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]));

        let parameters = advertising_parameters(0x00A0);
        assert_eq!(parameters.len(), 15);
        assert_eq!(&parameters[..4], &[0xA0, 0x00, 0xA0, 0x00]);
        assert_eq!(parameters[4], 0x00, "ADV_IND");
        assert_eq!(parameters[5], 0x01, "own address random");
        assert_eq!(parameters[13], 0x07, "all channels");

        let data = advertising_data("bleradar-beacon").unwrap();
        assert_eq!(data.len(), 32);
        assert_eq!(data[0], 20, "3 flag bytes + 2 + 15 name bytes");
        assert_eq!(&data[1..4], &[0x02, 0x01, 0x06]);
        assert_eq!(data[4], 16);
        assert_eq!(data[5], 0x09);
        assert_eq!(&data[6..21], b"bleradar-beacon");
        assert!(data[21..].iter().all(|byte| *byte == 0));
        assert_eq!(advertising_data("").unwrap()[0], 5);
        assert!(advertising_data(&"x".repeat(26)).is_ok());
        assert!(advertising_data(&"x".repeat(27)).is_err());
    }

    #[test]
    fn the_advertiser_is_driven_command_by_command() {
        // An unrelated LE meta event and another command's completion
        // arrive before the first answer and are skipped.
        let noise = vec![
            0x04, 0x3E, 0x03, 0x02, 0x01, 0x00, // LE meta event
            0x04, 0x0E, 0x04, 0x01, 0x00, 0x00, 0x00, // Command Complete for NOP
        ];
        let (port, controller_thread) = fake_controller(0x00, noise);
        let mut controller = Controller::connect(port, TIMEOUT).unwrap();
        controller.reset().unwrap();
        assert_eq!(controller.read_bd_addr().unwrap(), "01:02:03:04:05:06");
        controller
            .advertise("C0:DE:BE:AC:0D:01", "bleradar-beacon", 0x00A0)
            .unwrap();
        assert!(
            controller
                .advertise("11:22:33:44:55:66", "bleradar-beacon", 0x00A0)
                .unwrap_err()
                .contains("static random")
        );
        controller.stop_advertising().unwrap();
        drop(controller);
        let seen = controller_thread.join().unwrap();
        let opcodes: Vec<u16> = seen.iter().map(|(opcode, _)| *opcode).collect();
        assert_eq!(
            opcodes,
            vec![
                RESET,
                READ_BD_ADDR,
                LE_SET_RANDOM_ADDRESS,
                LE_SET_ADVERTISING_PARAMETERS,
                LE_SET_ADVERTISING_DATA,
                LE_SET_ADVERTISING_ENABLE,
                LE_SET_ADVERTISING_ENABLE,
            ]
        );
        assert_eq!(seen[2].1, vec![0x01, 0x0D, 0xAC, 0xBE, 0xDE, 0xC0]);
        assert_eq!(seen[3].1, advertising_parameters(0x00A0));
        assert_eq!(seen[4].1, advertising_data("bleradar-beacon").unwrap());
        assert_eq!(seen[5].1, vec![0x01]);
        assert_eq!(seen[6].1, vec![0x00]);
    }

    #[test]
    fn a_refused_command_names_its_status() {
        let (port, controller_thread) = fake_controller(0x0C, Vec::new());
        let mut controller = Controller::connect(port, TIMEOUT).unwrap();
        let error = controller.reset().unwrap_err();
        assert!(
            error.contains("HCI_Reset: status 0x0C (Command Disallowed)"),
            "{error}"
        );
        drop(controller);
        assert_eq!(controller_thread.join().unwrap().len(), 1);
        assert!(
            Controller::connect(1, Duration::from_millis(200))
                .unwrap_err()
                .contains("connecting to 127.0.0.1:1")
        );
    }
}
