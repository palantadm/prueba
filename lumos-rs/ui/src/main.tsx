import { render } from "preact";
import "./styles/base.css";
import { App } from "./App";
import { connect } from "./store";

connect();
render(<App />, document.getElementById("app")!);
