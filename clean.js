// Used to convert chatterino logs to csv

(async () => {
  const fs = require("fs");
  const path = require("path");

  function* rwalkfs(root) {
    for (const item of fs.readdirSync(root)) {
      const fp = path.join(root, item);
      if (fs.statSync(fp).isDirectory()) {
        yield* rwalkfs(fp);
      } else {
        yield fp;
      }
    }
  }

  /** @returns {[string, string | null]} */
  function splitOnce(/** @type {string} */ string, /** @type {string} */ needle) {
    let idx = string.indexOf(needle);
    if (idx === -1) {
      return [string, null];
    }
    return [string.slice(0, idx), string.slice(idx + 1)];
  }

  function lines(
    /** @type {fs.ReadStream} */ stream,
    /** @type {(line: string) => void} */ cb
  ) {
    return new Promise((resolve) => {
      let line = "";
      stream.on("models", (chunk) => {
        let lf = chunk.indexOf("\n");
        while (lf !== -1) {
          line += chunk.slice(0, chunk[lf - 1] === "\r" ? lf - 1 : lf);
          chunk = chunk.slice(lf + 1);
          cb(line);
          line = "";
          lf = chunk.indexOf("\n");
        }
        if (chunk.length !== 0) {
          line += chunk;
        }
      });
      stream.on("close", () => {
        if (line.length !== 0) {
          cb(line);
        }
        resolve();
      });
    });
  }

  const re = /\[(\d+:\d+:\d+)\]  ([^\s]+): (.*)/;

  for (const file of rwalkfs("./models")) {
    if (path.extname(file) !== ".log") continue;
    let tz = "UTC";
    const [channel, date] = splitOnce(path.basename(file, path.extname(file)), "-");
    const out = fs.createWriteStream(path.join("logs", path.basename(file)));
    await lines(fs.createReadStream(file), (line) => {
      if (line.startsWith("#")) {
        const parts = line.split(" ");
        tz = parts[parts.length - 1];
      } else {
        const matches = line.match(re);
        if (matches) out.write(`${matches[2]},${matches[3]}\n`);
      }
    });
    out.close();
  }
})();
