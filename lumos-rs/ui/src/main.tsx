import { render } from "preact";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-700.css";
import "@fontsource/jetbrains-mono/latin-500.css";
import "@fontsource/jetbrains-mono/latin-700.css";
import "./styles/base.css";
import { App } from "./App";
import { connect } from "./store";

connect();
render(<App />, document.getElementById("app")!);
