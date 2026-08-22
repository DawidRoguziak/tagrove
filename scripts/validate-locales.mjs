import { validateAllLocales } from "./locale-contract.mjs";

const { codes, errors } = await validateAllLocales();
if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`Locale contract valid for ${codes.length} languages.`);
}
