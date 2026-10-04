import { mountApplication } from "./mountApplication";
import "../styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing application root");
const dispose = mountApplication(root);
import.meta.hot?.dispose(dispose);
