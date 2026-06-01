use boltffi::__private::FfiBuf;
use boltffi_core::wire::WireDecode;
use boltffi_tests::{FixturePoint, FixtureStringConfig};

fn decode_buf<T: WireDecode>(buf: &FfiBuf) -> T {
    let (result, _) = T::decode_from(unsafe { buf.as_byte_slice() }).unwrap();
    result
}

unsafe extern "C" {
    fn boltffi_fixture_point_origin() -> FixturePoint;
    fn boltffi_fixture_point_new_at(x: f64, y: f64) -> FixturePoint;
    fn boltffi_fixture_point_distance_to_origin(self_value: FixturePoint) -> f64;
    fn boltffi_fixture_point_scale(self_value: FixturePoint, factor: f64) -> FixturePoint;
    fn boltffi_fixture_point_midpoint_to(a: FixturePoint, b: FixturePoint) -> FixturePoint;
    fn boltffi_fixture_string_config_from_owned_name(
        name_ptr: *const u8,
        name_len: usize,
    ) -> FfiBuf;
    fn boltffi_fixture_string_config_from_borrowed_name(
        name_ptr: *const u8,
        name_len: usize,
    ) -> FfiBuf;
}

mod constructors {
    use super::*;

    #[test]
    fn origin_returns_zero_point() {
        let point = unsafe { boltffi_fixture_point_origin() };
        assert_eq!(point, FixturePoint { x: 0.0, y: 0.0 });
    }

    #[test]
    fn new_at_returns_specified_coordinates() {
        let point = unsafe { boltffi_fixture_point_new_at(3.0, 4.0) };
        assert_eq!(point, FixturePoint { x: 3.0, y: 4.0 });
    }

    #[test]
    fn owned_string_constructor_returns_wire_encoded_record() {
        let name = "owned config";
        let buf =
            unsafe { boltffi_fixture_string_config_from_owned_name(name.as_ptr(), name.len()) };
        let config: FixtureStringConfig = decode_buf(&buf);
        assert_eq!(
            config,
            FixtureStringConfig {
                name: name.to_string(),
                source: "owned".to_string(),
            }
        );
    }

    #[test]
    fn borrowed_string_constructor_returns_wire_encoded_record() {
        let name = "borrowed config";
        let buf =
            unsafe { boltffi_fixture_string_config_from_borrowed_name(name.as_ptr(), name.len()) };
        let config: FixtureStringConfig = decode_buf(&buf);
        assert_eq!(
            config,
            FixtureStringConfig {
                name: name.to_string(),
                source: "borrowed".to_string(),
            }
        );
    }
}

mod instance_methods {
    use super::*;

    #[test]
    fn distance_to_origin_computes_correctly() {
        let point = FixturePoint { x: 3.0, y: 4.0 };
        let distance = unsafe { boltffi_fixture_point_distance_to_origin(point) };
        assert!((distance - 5.0).abs() < 1e-10);
    }

    #[test]
    fn distance_of_origin_is_zero() {
        let point = FixturePoint { x: 0.0, y: 0.0 };
        let distance = unsafe { boltffi_fixture_point_distance_to_origin(point) };
        assert!((distance - 0.0).abs() < 1e-10);
    }
}

mod mut_instance_methods {
    use super::*;

    #[test]
    fn scale_returns_mutated_point() {
        let point = FixturePoint { x: 2.0, y: 3.0 };
        let scaled = unsafe { boltffi_fixture_point_scale(point, 2.0) };
        assert_eq!(scaled, FixturePoint { x: 4.0, y: 6.0 });
    }

    #[test]
    fn scale_by_zero_returns_zero_point() {
        let point = FixturePoint { x: 5.0, y: 10.0 };
        let scaled = unsafe { boltffi_fixture_point_scale(point, 0.0) };
        assert_eq!(scaled, FixturePoint { x: 0.0, y: 0.0 });
    }

    #[test]
    fn scale_by_negative_flips_signs() {
        let point = FixturePoint { x: 1.0, y: 2.0 };
        let scaled = unsafe { boltffi_fixture_point_scale(point, -1.0) };
        assert_eq!(scaled, FixturePoint { x: -1.0, y: -2.0 });
    }
}

mod static_methods {
    use super::*;

    #[test]
    fn midpoint_computes_correctly() {
        let a = FixturePoint { x: 0.0, y: 0.0 };
        let b = FixturePoint { x: 4.0, y: 6.0 };
        let mid = unsafe { boltffi_fixture_point_midpoint_to(a, b) };
        assert_eq!(mid, FixturePoint { x: 2.0, y: 3.0 });
    }

    #[test]
    fn midpoint_of_same_point_is_that_point() {
        let p = FixturePoint { x: 3.0, y: 7.0 };
        let mid = unsafe { boltffi_fixture_point_midpoint_to(p, p) };
        assert_eq!(mid, p);
    }
}

mod roundtrip {
    use super::*;

    #[test]
    fn constructor_then_method_roundtrip() {
        let point = unsafe { boltffi_fixture_point_new_at(6.0, 8.0) };
        let distance = unsafe { boltffi_fixture_point_distance_to_origin(point) };
        assert!((distance - 10.0).abs() < 1e-10);
    }

    #[test]
    fn constructor_then_scale_roundtrip() {
        let point = unsafe { boltffi_fixture_point_new_at(1.0, 2.0) };
        let scaled = unsafe { boltffi_fixture_point_scale(point, 3.0) };
        let distance = unsafe { boltffi_fixture_point_distance_to_origin(scaled) };
        let expected = (9.0f64 + 36.0).sqrt();
        assert!((distance - expected).abs() < 1e-10);
    }
}
