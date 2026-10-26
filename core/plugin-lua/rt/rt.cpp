// The monotonic clock libc++ declares; the Luau `require` timeout is its only user.
#include <chrono>

namespace std {
inline namespace __1 {
namespace chrono {
steady_clock::time_point steady_clock::now() noexcept { return time_point(duration(0)); }
} // namespace chrono
} // namespace __1
} // namespace std
