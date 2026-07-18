import Demo
import XCTest

final class CancellableTests: DemoTestCase {
    func testCancellableCounterProgressAndCountTo() async throws {
        let counter = CancellableCounter.new()
        XCTAssertEqual(counter.progress(), 0)
        let total = await counter.countTo(target: 3, tickMillis: 10)
        XCTAssertGreaterThanOrEqual(total, 3)
        XCTAssertGreaterThanOrEqual(counter.progress(), 3)
    }
}
