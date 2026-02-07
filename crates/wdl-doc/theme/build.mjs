import * as esbuild from 'esbuild';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const distDir = path.join(__dirname, "dist")
const webCommonDir = path.join(__dirname, "..", "..", "..", "web-common");
const webCommonDistDir = path.join(webCommonDir, "dist");

try {
    console.log(`Running 'npm install' in: ${webCommonDir}`);
    execSync('npm install', {
        cwd: webCommonDir,
        stdio: 'inherit'
    });

    console.log(`Running 'npm install' in: ${__dirname}`);
    execSync('npm install', {
        cwd: __dirname,
        stdio: 'inherit'
    });

    console.log(`Running 'npm build' in: ${webCommonDir}`);
    execSync('npm run build', {
        cwd: webCommonDir,
        stdio: 'inherit'
    });
} catch (error) {
    process.exit(1);
}

await esbuild.build({
    entryPoints: ['./src/index.mjs'],
    bundle: true,
    outfile: path.join(distDir, "index.js"),
    format: 'esm',
    minify: true,
    loader: {
        '.json': 'json',
    },
    alias: {
        'common.js': path.join(webCommonDistDir, 'common.js')
    }
}).catch(() => process.exit(1));
