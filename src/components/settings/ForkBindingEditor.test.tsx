// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ForkBindingEditor } from "./ForkBindingEditor";

afterEach(cleanup);

it("requires opt-in and captures a keyboard binding for a selected diagram control", () => {
  const onChange = vi.fn();
  render(<ForkBindingEditor value="" onChange={onChange} />);
  expect(screen.queryByRole("button", { name: "Click to bind" })).toBeNull();
  fireEvent.click(screen.getByRole("checkbox", { name: /Use KytyPS5 fork/ }));
  fireEvent.click(screen.getByRole("button", { name: "Circle / east" }));
  fireEvent.click(screen.getByRole("button", { name: "Click to bind" }));
  fireEvent.keyDown(window, { key: "j" });
  expect(onChange).toHaveBeenCalledWith("Circle=J");
});

it("captures mouse buttons and rejects reserved keyboard keys", () => {
  const onChange = vi.fn();
  render(<ForkBindingEditor value="" onChange={onChange} />);
  fireEvent.click(screen.getByRole("checkbox", { name: /Use KytyPS5 fork/ }));
  fireEvent.click(screen.getByRole("button", { name: "Click to bind" }));
  fireEvent.keyDown(window, { key: "F1" });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.mouseDown(window, { button: 2 });
  expect(onChange).toHaveBeenCalledWith("Cross=Mouse:Right");
});

it("keeps right and left modifier keys distinct", () => {
  const onChange = vi.fn();
  render(<ForkBindingEditor value="" onChange={onChange} />);
  fireEvent.click(screen.getByRole("checkbox", { name: /Use KytyPS5 fork/ }));
  fireEvent.click(screen.getByRole("button", { name: "Click to bind" }));
  fireEvent.keyDown(window, { key: "Shift", location: 2 });
  expect(onChange).toHaveBeenCalledWith("Cross=Right Shift");
});
