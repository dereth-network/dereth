/* The minimal C library the Luau sources and the C++ runtime need in wasm32-unknown-unknown.
 * Memory allocation, strtod and floating-point formatting are in Rust (shim.rs). */
#include <stddef.h>
#include <stdint.h>
#include <stdarg.h>
#include <limits.h>
#include <time.h>

int errno;

/* ---- strings ---- */
int strcmp(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}
int strncmp(const char *a, const char *b, size_t n) {
    for (; n; n--, a++, b++) {
        if (*a != *b) return (unsigned char)*a - (unsigned char)*b;
        if (!*a) return 0;
    }
    return 0;
}
char *strchr(const char *s, int c) {
    for (;; s++) {
        if (*s == (char)c) return (char *)s;
        if (!*s) return 0;
    }
}
char *strrchr(const char *s, int c) {
    const char *r = 0;
    for (;; s++) {
        if (*s == (char)c) r = s;
        if (!*s) return (char *)r;
    }
}
void *memchr(const void *s, int c, size_t n) {
    const unsigned char *p = s;
    for (; n; n--, p++) if (*p == (unsigned char)c) return (void *)p;
    return 0;
}
size_t strspn(const char *s, const char *acc) {
    size_t n = 0;
    while (s[n] && strchr(acc, s[n])) n++;
    return n;
}
size_t strcspn(const char *s, const char *rej) {
    size_t n = 0;
    while (s[n] && !strchr(rej, s[n])) n++;
    return n;
}
char *strpbrk(const char *s, const char *acc) {
    s += strcspn(s, acc);
    return *s ? (char *)s : 0;
}
char *strstr(const char *h, const char *n) {
    if (!*n) return (char *)h;
    for (; *h; h++) {
        const char *a = h, *b = n;
        while (*a && *a == *b) { a++; b++; }
        if (!*b) return (char *)h;
    }
    return 0;
}
size_t strlen(const char *s);
char *strcpy(char *d, const char *s) { char *r = d; while ((*d++ = *s++)); return r; }
char *strncpy(char *d, const char *s, size_t n) {
    char *r = d;
    for (; n && *s; n--) *d++ = *s++;
    for (; n; n--) *d++ = 0;
    return r;
}
char *strcat(char *d, const char *s) { strcpy(d + strlen(d), s); return d; }
char *strncat(char *d, const char *s, size_t n) {
    char *r = d;
    d += strlen(d);
    while (n-- && *s) *d++ = *s++;
    *d = 0;
    return r;
}

/* ---- character classes ---- */
int isdigit(int c) { return (unsigned)c - '0' < 10; }
int islower(int c) { return (unsigned)c - 'a' < 26; }
int isupper(int c) { return (unsigned)c - 'A' < 26; }
int isalpha(int c) { return islower(c) || isupper(c); }
int isalnum(int c) { return isalpha(c) || isdigit(c); }
int isxdigit(int c) { return isdigit(c) || (unsigned)(c | 32) - 'a' < 6; }
int isspace(int c) { return c == ' ' || (unsigned)c - '\t' < 5; }
int iscntrl(int c) { return (unsigned)c < 0x20 || c == 0x7f; }
int isgraph(int c) { return (unsigned)c - 0x21 < 0x5e; }
int isprint(int c) { return (unsigned)c - 0x20 < 0x5f; }
int ispunct(int c) { return isgraph(c) && !isalnum(c); }
int tolower(int c) { return isupper(c) ? c | 32 : c; }
int toupper(int c) { return islower(c) ? c & 0x5f : c; }

/* ---- integers from text ---- */
static unsigned long long scan_uint(const char *s, char **end, int base, int *neg, int *any) {
    const char *p = s;
    unsigned long long v = 0;
    while (isspace(*p)) p++;
    *neg = 0;
    if (*p == '+' || *p == '-') *neg = *p++ == '-';
    if ((base == 0 || base == 16) && p[0] == '0' && (p[1] | 32) == 'x' && isxdigit(p[2])) {
        p += 2;
        base = 16;
    } else if (base == 0) {
        base = p[0] == '0' ? 8 : 10;
    }
    *any = 0;
    for (;; p++) {
        int d;
        if (isdigit(*p)) d = *p - '0';
        else if (isalpha(*p)) d = (*p | 32) - 'a' + 10;
        else break;
        if (d >= base) break;
        v = v * base + d;
        *any = 1;
    }
    if (end) *end = (char *)(*any ? p : s);
    return v;
}
unsigned long long strtoull(const char *s, char **end, int base) {
    int neg, any;
    unsigned long long v = scan_uint(s, end, base, &neg, &any);
    return neg ? -v : v;
}
unsigned long strtoul(const char *s, char **end, int base) { return (unsigned long)strtoull(s, end, base); }
long long strtoll(const char *s, char **end, int base) {
    int neg, any;
    unsigned long long v = scan_uint(s, end, base, &neg, &any);
    return neg ? -(long long)v : (long long)v;
}
long strtol(const char *s, char **end, int base) { return (long)strtoll(s, end, base); }
int atoi(const char *s) { return (int)strtol(s, 0, 10); }
void abort(void) { __builtin_trap(); }

/* ---- mathematics ---- */
double modf(double x, double *ip) {
    double i = __builtin_trunc(x);
    *ip = i;
    if (__builtin_isinf(x)) return __builtin_copysign(0.0, x);
    return __builtin_copysign(x - i, x);
}
double frexp(double x, int *e) {
    union { double d; uint64_t u; } y = { x };
    int ee = (int)(y.u >> 52 & 0x7ff);
    if (!ee) {
        if (x) {
            x = frexp(x * 0x1p64, e);
            *e -= 64;
        } else *e = 0;
        return x;
    } else if (ee == 0x7ff) {
        return x;
    }
    *e = ee - 0x3fe;
    y.u &= 0x800fffffffffffffull;
    y.u |= 0x3fe0000000000000ull;
    return y.d;
}

/* ---- time: the host supplies time; the sandbox replaces os.time and os.clock ---- */
time_t time(time_t *t) { if (t) *t = 0; return 0; }
clock_t clock(void) { return 0; }
double difftime(time_t a, time_t b) { return (double)a - (double)b; }
struct tm *gmtime_r(const time_t *t, struct tm *tm) {
    long long secs = *t, days = secs / 86400, rem = secs % 86400;
    if (rem < 0) { rem += 86400; days--; }
    tm->tm_hour = (int)(rem / 3600);
    tm->tm_min = (int)(rem / 60 % 60);
    tm->tm_sec = (int)(rem % 60);
    tm->tm_wday = (int)((4 + days) % 7 + 7) % 7;
    /* civil_from_days (H. Hinnant) */
    long long z = days + 719468, era = (z >= 0 ? z : z - 146096) / 146097;
    unsigned doe = (unsigned)(z - era * 146097);
    unsigned yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    long long y = (long long)yoe + era * 400;
    unsigned doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    unsigned mp = (5 * doy + 2) / 153;
    unsigned d = doy - (153 * mp + 2) / 5 + 1;
    unsigned m = mp < 10 ? mp + 3 : mp - 9;
    y += m <= 2;
    tm->tm_year = (int)(y - 1900);
    tm->tm_mon = (int)m - 1;
    tm->tm_mday = (int)d;
    int leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    static const int cum[12] = {0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334};
    tm->tm_yday = cum[tm->tm_mon] + tm->tm_mday - 1 + (leap && tm->tm_mon > 1);
    tm->tm_isdst = 0;
    return tm;
}
struct tm *localtime_r(const time_t *t, struct tm *tm) { return gmtime_r(t, tm); }
time_t mktime(struct tm *tm) { (void)tm; return 0; }
size_t strftime(char *s, size_t max, const char *fmt, const struct tm *tm) {
    (void)fmt; (void)tm;
    if (max) s[0] = 0;
    return 0;
}

/* ---- formatting; the console output streams go nowhere ---- */
typedef struct { int unused; } FILE_;
static FILE_ out_stream, err_stream;
void *stdout = &out_stream;
void *stderr = &err_stream;
size_t fwrite(const void *p, size_t sz, size_t n, void *f) { (void)p; (void)f; return n; (void)sz; }
int fputc(int c, void *f) { (void)f; return c; }
int fputs(const char *s, void *f) { (void)s; (void)f; return 0; }
int fflush(void *f) { (void)f; return 0; }
int vfprintf(void *f, const char *fmt, va_list ap) { (void)f; (void)fmt; (void)ap; return 0; }
int fprintf(void *f, const char *fmt, ...) { (void)f; (void)fmt; return 0; }

/* Formats `v` for conversion `conv` (f, e, g, a and their capitals) with `prec` digits into `buf`,
 * without sign or padding; `alt` is the `#` flag. Returns the length. */
size_t __luaurt_fmt_f64(char *buf, size_t cap, double v, int conv, int prec, int alt);

struct out { char *buf; size_t cap, len; };
static void put(struct out *o, char c) {
    if (o->len + 1 < o->cap) o->buf[o->len] = c;
    o->len++;
}
static void pad(struct out *o, char c, int n) { while (n-- > 0) put(o, c); }

int vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap) {
    struct out o = { buf, cap, 0 };
    for (; *fmt; fmt++) {
        if (*fmt != '%') { put(&o, *fmt); continue; }
        fmt++;
        int left = 0, plus = 0, space = 0, alt = 0, zero = 0, width = 0, prec = -1, lng = 0;
        for (;; fmt++) {
            if (*fmt == '-') left = 1;
            else if (*fmt == '+') plus = 1;
            else if (*fmt == ' ') space = 1;
            else if (*fmt == '#') alt = 1;
            else if (*fmt == '0') zero = 1;
            else break;
        }
        if (*fmt == '*') { width = va_arg(ap, int); if (width < 0) { left = 1; width = -width; } fmt++; }
        else while (isdigit(*fmt)) width = width * 10 + (*fmt++ - '0');
        if (*fmt == '.') {
            fmt++;
            prec = 0;
            if (*fmt == '*') { prec = va_arg(ap, int); fmt++; }
            else while (isdigit(*fmt)) prec = prec * 10 + (*fmt++ - '0');
        }
        for (;; fmt++) {
            if (*fmt == 'l') lng++;
            else if (*fmt == 'h' || *fmt == 'L') {}
            else if (*fmt == 'z' || *fmt == 't') lng = 0; /* size_t is 32 bits here */
            else if (*fmt == 'j') lng = 2;
            else break;
        }
        char tmp[512], sign = 0;
        const char *body = tmp;
        int blen = 0, numeric = 1, pre_zeros = 0;
        char c = *fmt;
        switch (c) {
        case 'd': case 'i': case 'u': case 'x': case 'X': case 'o': case 'p': {
            unsigned long long u;
            if (c == 'p') { u = (uintptr_t)va_arg(ap, void *); alt = 1; }
            else if (c == 'd' || c == 'i') {
                long long s = lng >= 2 ? va_arg(ap, long long) : lng == 1 ? va_arg(ap, long) : va_arg(ap, int);
                if (s < 0) { sign = '-'; u = -(unsigned long long)s; }
                else { u = (unsigned long long)s; sign = plus ? '+' : space ? ' ' : 0; }
            } else {
                u = lng >= 2 ? va_arg(ap, unsigned long long) : lng == 1 ? va_arg(ap, unsigned long) : va_arg(ap, unsigned);
            }
            int base = (c == 'x' || c == 'X' || c == 'p') ? 16 : c == 'o' ? 8 : 10;
            const char *dig = c == 'X' ? "0123456789ABCDEF" : "0123456789abcdef";
            char rev[32];
            int n = 0;
            while (u) { rev[n++] = dig[u % base]; u /= base; }
            if (n == 0 && prec != 0) rev[n++] = '0';
            int k = 0;
            if (alt && base == 16 && n) { tmp[k++] = '0'; tmp[k++] = c == 'X' ? 'X' : 'x'; }
            if (alt && base == 8) tmp[k++] = '0';
            if (prec > n) pre_zeros = prec - n;
            while (pre_zeros-- > 0) tmp[k++] = '0';
            while (n) tmp[k++] = rev[--n];
            blen = k;
            if (prec >= 0) zero = 0;
            break;
        }
        case 'f': case 'F': case 'e': case 'E': case 'g': case 'G': case 'a': case 'A': {
            double v = va_arg(ap, double);
            if (__builtin_signbit(v)) { sign = '-'; v = -v; }
            else sign = plus ? '+' : space ? ' ' : 0;
            blen = (int)__luaurt_fmt_f64(tmp, sizeof tmp, v, c, prec < 0 ? 6 : prec, alt);
            if (__builtin_isinf(v) || __builtin_isnan(v)) zero = 0;
            break;
        }
        case 'c': tmp[0] = (char)va_arg(ap, int); blen = 1; numeric = 0; break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s) s = "(null)";
            body = s;
            blen = 0;
            while (s[blen] && (prec < 0 || blen < prec)) blen++;
            numeric = 0;
            break;
        }
        case '%': tmp[0] = '%'; blen = 1; numeric = 0; break;
        default: tmp[0] = '%'; tmp[1] = c; blen = 2; numeric = 0; if (!c) fmt--; break;
        }
        int total = blen + (sign ? 1 : 0);
        if (!left && !(zero && numeric)) pad(&o, ' ', width - total);
        if (sign) put(&o, sign);
        if (!left && zero && numeric) pad(&o, '0', width - total);
        for (int i = 0; i < blen; i++) put(&o, body[i]);
        if (left) pad(&o, ' ', width - total);
    }
    if (cap) buf[o.len < cap ? o.len : cap - 1] = 0;
    return (int)o.len;
}
int snprintf(char *buf, size_t cap, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, cap, fmt, ap);
    va_end(ap);
    return n;
}
int sprintf(char *buf, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, (size_t)INT_MAX, fmt, ap);
    va_end(ap);
    return n;
}

/* ---- static constructors ----
 * The Luau sources have static constructors. Calling them from here, once, before the first VM,
 * makes the linker leave the module's exports alone (it would otherwise run the constructors on
 * every exported call). */
void __wasm_call_ctors(void);
void __luaurt_init(void) {
    static int done;
    if (!done) {
        done = 1;
        __wasm_call_ctors();
    }
}

/* ---- C++ runtime support ---- */
/* The terminate handler's message names the exception type undemangled. */
char *__cxa_demangle(const char *name, char *buf, size_t *n, int *status) {
    (void)name; (void)buf; (void)n;
    if (status) *status = -2;
    return 0;
}
int __cxa_atexit(void (*f)(void *), void *p, void *d) { (void)f; (void)p; (void)d; return 0; }
