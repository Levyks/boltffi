import Demo
import XCTest

final class KeywordFieldsRecordsTests: DemoTestCase {
    func testTypedEventFns() {
        let event = TypedEvent(id: 99, type: "circle")

        demoCase("case:records.keyword_fields.typed_event.should_roundtrip_raw_identifier_field")
        XCTAssertEqual(echoTypedEvent(event: event), event)
    }
}