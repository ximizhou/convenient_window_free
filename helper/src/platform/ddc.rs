use anyhow::{ensure, Result};

pub(super) fn brightness_query() -> [u8; 5] {
    let mut packet = [0x51, 0x82, 0x01, 0x10, 0];
    packet[4] = packet[..4].iter().fold(0x6e, |sum, byte| sum ^ byte);
    packet
}

pub(super) fn brightness_write(value: u16) -> [u8; 7] {
    let [high, low] = value.to_be_bytes();
    let mut packet = [0x51, 0x84, 0x03, 0x10, high, low, 0];
    packet[6] = packet[..6].iter().fold(0x6e, |sum, byte| sum ^ byte);
    packet
}

pub(super) fn brightness_reply(reply: &[u8]) -> Result<(u32, u32)> {
    ensure!(
        reply.len() == 11
            && reply[0] == 0x6e
            && reply[1] == 0x88
            && reply[2] == 0x02
            && reply[3] == 0
            && reply[4] == 0x10
            && reply[5] == 0,
        "显示器未返回有效的连续亮度读数"
    );
    ensure!(
        reply.iter().fold(0x50, |sum, byte| sum ^ byte) == 0,
        "DDC 亮度响应校验失败"
    );
    let max = u32::from(u16::from_be_bytes([reply[6], reply[7]]));
    let current = u32::from(u16::from_be_bytes([reply[8], reply[9]]));
    ensure!(max > 0 && current <= max, "显示器返回无效亮度范围");
    Ok((current, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_encode_the_brightness_opcode_and_big_endian_values() {
        assert_eq!(brightness_query(), [0x51, 0x82, 0x01, 0x10, 0xac]);
        let packet = brightness_write(0x1234);
        assert_eq!(&packet[..6], &[0x51, 0x84, 0x03, 0x10, 0x12, 0x34]);
        assert_eq!(packet.iter().fold(0x6e, |sum, byte| sum ^ byte), 0);
    }

    #[test]
    fn replies_validate_checksum_feature_type_and_range() {
        let mut reply = [0x6e, 0x88, 2, 0, 0x10, 0, 1, 0, 0, 128, 0];
        reply[10] = reply[..10].iter().fold(0x50, |sum, byte| sum ^ byte);
        assert_eq!(brightness_reply(&reply).unwrap(), (128, 256));
        for index in [0, 1, 2, 3, 4, 5, 10] {
            let mut bad = reply;
            bad[index] ^= 1;
            assert!(brightness_reply(&bad).is_err());
        }
        assert!(brightness_reply(&reply[..10]).is_err());
        reply[8] = 2;
        reply[10] = reply[..10].iter().fold(0x50, |sum, byte| sum ^ byte);
        assert!(brightness_reply(&reply).is_err());
    }
}
