// Used to convert chatterino logs to csv

(async () => {
  const fs = require("fs");
  const path = require("path");

  function ensure(dir) {
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

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
  function splitOnce(
    /** @type {string} */ string,
    /** @type {string} */ needle
  ) {
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
      stream.on("data", (chunk) => {
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

  for (const file of rwalkfs("./data")) {
    if (path.extname(file) !== ".log") continue;
    const [channel] = path.basename(file, path.extname(file)).split("-");
    const base = path.join("logs", channel);
    ensure(base);
    const out = fs.createWriteStream(path.join(base, path.basename(file)));
    await lines(fs.createReadStream(file), (line) => {
      const matches = line.match(re);
      if (matches) out.write(`${matches[2]},${matches[3]}\n`);
    });
    out.close();
  }
})();
