from tests.support import DemoTestCase

import demo


class ErrorEnumResultTests(DemoTestCase):
    def test_data_enum_returns(self) -> None:
        self.demo_case("case:results.error_enums.process_value.should_return_success_variant")
        self.assertEqual(demo.process_value(3), demo.ApiResultSuccess())
        self.demo_case("case:results.error_enums.process_value.should_return_error_code_variant")
        self.assertEqual(demo.process_value(0), demo.ApiResultErrorCode(-1))
        self.demo_case("case:results.error_enums.process_value.should_return_error_with_data_variant")
        self.assertEqual(demo.process_value(-3), demo.ApiResultErrorWithData(-3, -6))
        self.demo_case("case:results.error_enums.api_result_is_success.should_report_success_variant")
        self.assertIs(demo.api_result_is_success(demo.ApiResultSuccess()), True)
        self.demo_case("case:results.error_enums.api_result_is_success.should_report_error_variant")
        self.assertIs(demo.api_result_is_success(demo.ApiResultErrorCode(-1)), False)

    def test_success_response(self) -> None:
        point = demo.DataPoint(1.0, 2.0, 3)

        self.demo_case("case:results.error_enums.benchmark_response.should_make_success_response")
        self.assertEqual(demo.create_success_response(7, point), demo.BenchmarkResponse(7, (True, point)))
        self.demo_case("case:results.error_enums.benchmark_response.should_make_error_response")
        error = demo.ComputeErrorOverflow(-3, 0)
        error_envelope = demo.create_error_response(12, error)
        self.assertEqual(error_envelope, demo.BenchmarkResponse(12, (False, error)))
        success_envelope = demo.BenchmarkResponse(11, (True, demo.DataPoint(4.0, 5.0, 6)))
        self.demo_case("case:results.error_enums.benchmark_response.should_report_success_response")
        self.assertIs(demo.is_response_success(success_envelope), True)
        self.demo_case("case:results.error_enums.benchmark_response.should_report_error_response")
        self.assertIs(demo.is_response_success(error_envelope), False)
        self.demo_case("case:results.error_enums.benchmark_response.should_return_value_for_success_response")
        self.assertEqual(demo.get_response_value(success_envelope), demo.DataPoint(4.0, 5.0, 6))
        self.demo_case("case:results.error_enums.benchmark_response.should_return_none_for_error_response")
        self.assertIsNone(demo.get_response_value(error_envelope))
