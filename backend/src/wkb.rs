//! Minimal WKB header parsing.

const EWKB_Z: u32 = 0x8000_0000;
const EWKB_M: u32 = 0x4000_0000;
const EWKB_SRID: u32 = 0x2000_0000;

/// Returns the GeoParquet geometry type name of a WKB or EWKB geometry, e.g. `Point Z`.
///
/// Handles both byte orders, ISO dimension codes (+1000 Z, +2000 M, +3000 ZM) and EWKB
/// dimension flags. Returns `None` if the header is not valid WKB.
pub fn geometry_type(wkb: &[u8]) -> Option<String> {
    let header: [u8; 4] = wkb.get(1..5)?.try_into().ok()?;
    let code = match wkb[0] {
        0 => u32::from_be_bytes(header),
        1 => u32::from_le_bytes(header),
        _ => return None,
    };
    let ewkb_z = code & EWKB_Z != 0;
    let ewkb_m = code & EWKB_M != 0;
    let code = code & !(EWKB_Z | EWKB_M | EWKB_SRID);
    let (iso_z, iso_m) = match code / 1000 {
        0 => (false, false),
        1 => (true, false),
        2 => (false, true),
        3 => (true, true),
        _ => return None,
    };
    if (ewkb_z || ewkb_m) && (iso_z || iso_m) {
        return None;
    }
    let name = match code % 1000 {
        1 => "Point",
        2 => "LineString",
        3 => "Polygon",
        4 => "MultiPoint",
        5 => "MultiLineString",
        6 => "MultiPolygon",
        7 => "GeometryCollection",
        _ => return None,
    };
    let suffix = match (iso_z || ewkb_z, iso_m || ewkb_m) {
        (false, false) => "",
        (true, false) => " Z",
        (false, true) => " M",
        (true, true) => " ZM",
    };
    Some(format!("{name}{suffix}"))
}

#[cfg(test)]
mod tests {
    fn header(little_endian: bool, code: u32) -> Vec<u8> {
        let mut bytes = vec![u8::from(little_endian)];
        if little_endian {
            bytes.extend_from_slice(&code.to_le_bytes());
        } else {
            bytes.extend_from_slice(&code.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn wkb_point_le() {
        let mut wkb = header(true, 1);
        wkb.extend_from_slice(&1.0f64.to_le_bytes());
        wkb.extend_from_slice(&2.0f64.to_le_bytes());
        assert_eq!(super::geometry_type(&wkb).as_deref(), Some("Point"));
    }

    #[test]
    fn wkb_polygon_z_iso() {
        let mut wkb = header(true, 1003);
        wkb.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(super::geometry_type(&wkb).as_deref(), Some("Polygon Z"));
    }

    #[test]
    fn wkb_point_m_ewkb_be() {
        let mut wkb = header(false, 0x4000_0001);
        wkb.extend_from_slice(&1.0f64.to_be_bytes());
        wkb.extend_from_slice(&2.0f64.to_be_bytes());
        wkb.extend_from_slice(&3.0f64.to_be_bytes());
        assert_eq!(super::geometry_type(&wkb).as_deref(), Some("Point M"));
    }

    #[test]
    fn wkb_zm_codes() {
        assert_eq!(
            super::geometry_type(&header(true, 3006)).as_deref(),
            Some("MultiPolygon ZM")
        );
        assert_eq!(
            super::geometry_type(&header(false, 0xC000_0007)).as_deref(),
            Some("GeometryCollection ZM")
        );
        assert_eq!(
            super::geometry_type(&header(true, 0xA000_0002)).as_deref(),
            Some("LineString Z")
        );
    }

    #[test]
    fn wkb_garbage_is_none() {
        assert_eq!(super::geometry_type(&[]), None);
        assert_eq!(super::geometry_type(&[1, 1, 0]), None);
        assert_eq!(super::geometry_type(&[7, 1, 0, 0, 0]), None);
        assert_eq!(super::geometry_type(&header(true, 99)), None);
        assert_eq!(super::geometry_type(&header(true, 4001)), None);
        assert_eq!(super::geometry_type(&header(true, 0x8000_03E9)), None);
    }
}
