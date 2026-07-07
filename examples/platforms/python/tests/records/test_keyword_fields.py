from tests.support import DemoTestCase

import demo


class KeywordFieldRecordTests(DemoTestCase):
    def test_typed_event(self) -> None:
        event = demo.TypedEvent(99, type_="circle")

        self.demo_case("case:records.keyword_fields.typed_event.should_roundtrip_raw_identifier_field")
        self.assertEqual(demo.echo_typed_event(event), event)