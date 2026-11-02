(() => {
  const now = new Date()
  const values = {
    'Page hostname': location.hostname || '(extension page)',
    'navigator.language': navigator.language,
    'navigator.languages': navigator.languages.join(', '),
    'Intl locale': Intl.DateTimeFormat().resolvedOptions().locale,
    'Intl time zone': Intl.DateTimeFormat().resolvedOptions().timeZone,
    'Date offset (minutes)': String(now.getTimezoneOffset()),
    'Date local time': now.toLocaleString(),
    'Date native string (not covered)': now.toString(),
    'User Agent (not covered)': navigator.userAgent,
  }
  const result = document.querySelector('#results')
  for (const [label, value] of Object.entries(values)) {
    const term = document.createElement('dt')
    term.textContent = label
    const detail = document.createElement('dd')
    detail.textContent = value
    result.append(term, detail)
  }
})()
