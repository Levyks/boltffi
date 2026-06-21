from tests.support import DemoTestCase

import demo


class BasicResultTests(DemoTestCase):
    def test_result_parameter(self) -> None:
        self.demo_case("case:results.basic.result_to_string.should_render_ok")
        self.assertEqual(demo.result_to_string((True, 7)), "ok: 7")
        self.demo_case("case:results.basic.result_to_string.should_render_err")
        self.assertEqual(demo.result_to_string((False, "bad")), "err: bad")
