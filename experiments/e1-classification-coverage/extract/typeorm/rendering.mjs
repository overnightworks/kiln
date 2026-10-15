// JSONL rendering: one line per record, byte-stable.

const lines = (records) => records.map((record) => `${JSON.stringify(record)}\n`).join("");

const locationOf = ({ file, line, column }) => ({ file, line, column });

export function renderUnits(units) {
  return lines(
    units.map((unit) => ({
      subject: { application: unit.application },
      ...locationOf(unit),
      rule: unit.rule,
      parameters: unit.parameters,
      absorbed_sites: unit.absorbedSites.map(locationOf),
    })),
  );
}

export const renderReceiverEvidence = lines;

export function renderUnmatchedMarkers(markers) {
  return lines(markers.map(locationOf));
}

export const renderInstallScripts = lines;

export function renderGeneratedLocks(locks) {
  return lines(locks.map(({ file, sha256, installedAt }) => ({ file, sha256, installed_at: installedAt })));
}
