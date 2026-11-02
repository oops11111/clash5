(() => {
  const key = Symbol.for('clash5.browserPrivacy.apply')
  if (window[key]) return
  Object.defineProperty(window, key, { value(profile) {
    const originalDateTimeFormat = Intl.DateTimeFormat
    const language = profile.languages[0]
    const zone = profile.timeZone
    const optionsWithZone = (options) => ({ ...(options ?? {}), timeZone: options?.timeZone ?? zone })

    Object.defineProperty(Navigator.prototype, 'language', { configurable: true, get: () => language })
    Object.defineProperty(Navigator.prototype, 'languages', { configurable: true, get: () => [...profile.languages] })
    Intl.DateTimeFormat = new Proxy(originalDateTimeFormat, {
      apply(target, thisArg, args) {
        return Reflect.apply(target, thisArg, [args[0] ?? profile.languages, optionsWithZone(args[1])])
      },
      construct(target, args, newTarget) {
        return Reflect.construct(target, [args[0] ?? profile.languages, optionsWithZone(args[1])], newTarget)
      },
    })

    const clock = new originalDateTimeFormat('en-US', {
      timeZone: zone,
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23',
    })
    const nativeGetTime = Date.prototype.getTime
    const nativeUtc = Object.fromEntries(
      ['FullYear', 'Month', 'Date', 'Day', 'Hours', 'Minutes', 'Seconds', 'Milliseconds']
        .map((part) => [part, Date.prototype[`getUTC${part}`]]),
    )
    const offset = (date) => {
      const milliseconds = nativeGetTime.call(date)
      if (!Number.isFinite(milliseconds)) return NaN
      const parts = Object.fromEntries(clock.formatToParts(date)
        .filter((part) => part.type !== 'literal')
        .map((part) => [part.type, Number(part.value)]))
      const zoned = Date.UTC(parts.year, parts.month - 1, parts.day, parts.hour, parts.minute, parts.second)
      return (Math.floor(milliseconds / 1000) * 1000 - zoned) / 60000
    }
    Object.defineProperty(Date.prototype, 'getTimezoneOffset', {
      configurable: true, writable: true, value() { return offset(this) },
    })
    for (const [part, getter] of Object.entries(nativeUtc)) {
      Object.defineProperty(Date.prototype, `get${part}`, {
        configurable: true, writable: true,
        value() {
          const time = nativeGetTime.call(this)
          return getter.call(new Date(time - offset(this) * 60000))
        },
      })
    }
    Object.defineProperty(Date.prototype, 'getDay', {
      configurable: true, writable: true,
      value() {
        const time = nativeGetTime.call(this)
        return Date.prototype.getUTCDay.call(new Date(time - offset(this) * 60000))
      },
    })
    for (const method of ['toLocaleString', 'toLocaleDateString', 'toLocaleTimeString']) {
      const native = Date.prototype[method]
      Object.defineProperty(Date.prototype, method, {
        configurable: true, writable: true,
        value(locales, options) {
          return native.call(this, locales ?? profile.languages, optionsWithZone(options))
        },
      })
    }
  } })
})()
