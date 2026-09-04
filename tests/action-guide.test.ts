// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  activeWorkflowAction,
  destroyActionGuide,
  renderActionGuide,
  visibleTurnControl,
} from "../src/content/action-guide";
import { emptyResources } from "../src/core/resources";

const rect = {
  x: 20,
  y: 20,
  left: 20,
  top: 20,
  right: 100,
  bottom: 60,
  width: 80,
  height: 40,
  toJSON: () => ({}),
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("chrome", {
    runtime: {
      getURL: (path: string) => `chrome-extension://fixture/${path}`,
    },
  });
  vi.stubGlobal(
    "getComputedStyle",
    () =>
      ({
        display: "block",
        visibility: "visible",
        opacity: "1",
      }) as CSSStyleDeclaration,
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(
    rect as DOMRect,
  );
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 1200,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 800,
  });
});

afterEach(() => {
  destroyActionGuide();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("action guide autopilot", () => {
  it("clicks a recommended action without a private-room gate", async () => {
    const roll = document.createElement("button");
    roll.textContent = "Roll dice";
    const clicked = vi.fn();
    roll.addEventListener("click", clicked);
    document.body.append(roll);

    renderActionGuide(
      {
        kind: "turn-control",
        control: "roll",
        label: "Roll dice",
        signature: "roll",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(clicked).toHaveBeenCalledOnce();
  });

  it("honors a configured autopilot start delay before the first click", async () => {
    const roll = document.createElement("button");
    roll.textContent = "Roll dice";
    const clicked = vi.fn();
    roll.addEventListener("click", clicked);
    document.body.append(roll);

    renderActionGuide(
      {
        kind: "turn-control",
        control: "roll",
        label: "Roll dice",
        signature: "delayed-roll",
        confidence: 1,
      },
      { highlight: true, autonomous: true, autopilotDelayMs: 3_000 },
    );

    await vi.advanceTimersByTimeAsync(2_999);
    expect(clicked).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(clicked).toHaveBeenCalledOnce();
  });

  it("cancels a pending delayed click when autopilot turns off", async () => {
    const roll = document.createElement("button");
    roll.textContent = "Roll dice";
    const clicked = vi.fn();
    roll.addEventListener("click", clicked);
    document.body.append(roll);
    const action = {
      kind: "turn-control" as const,
      control: "roll" as const,
      label: "Roll dice",
      signature: "cancel-delayed-roll",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      autopilotDelayMs: 5_000,
    });
    await vi.advanceTimersByTimeAsync(1_000);
    renderActionGuide(action, { highlight: true, autonomous: false });
    await vi.advanceTimersByTimeAsync(5_000);

    expect(clicked).not.toHaveBeenCalled();
  });

  it("retries the same ordinary control after its first validation fails", () => {
    const roll = document.createElement("button");
    roll.textContent = "Roll dice";
    const clicked = vi.fn();
    roll.addEventListener("click", clicked);
    document.body.append(roll);
    let valid = false;
    const action = {
      kind: "turn-control" as const,
      control: "roll" as const,
      label: "Roll dice",
      signature: "replanned-roll",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => valid,
    });
    expect(clicked).not.toHaveBeenCalled();

    valid = true;
    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => valid,
    });

    expect(clicked).toHaveBeenCalledOnce();
  });

  it("retries the same board action after its first dispatch fails", () => {
    let failDispatch = true;
    const messages: unknown[] = [];
    vi.spyOn(window, "postMessage").mockImplementation((message) => {
      if (failDispatch) throw new Error("fixture dispatch failure");
      messages.push(message);
    });
    const action = {
      kind: "board" as const,
      boardAction: "road" as const,
      targetId: "e:1,0,0",
      point: { x: 240, y: 180 },
      label: "Place road here",
      signature: "replanned-board-road",
      confidence: 1,
    };

    expect(() =>
      renderActionGuide(action, {
        highlight: true,
        autonomous: true,
        validate: () => true,
      }),
    ).not.toThrow();
    expect(messages).toHaveLength(0);

    failDispatch = false;
    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => true,
    });

    expect(messages).toContainEqual({
      source: "colonist-assistant-content",
      type: "execute-board-action",
      action: "road",
      targetId: "e:1,0,0",
      signature: "replanned-board-road",
      attempt: 1,
    });
  });

  it("clicks an active dice face and never mistakes pass-turn for roll", async () => {
    const rollGroup = document.createElement("div");
    rollGroup.id = "roll-dice-button";
    const leftDie = document.createElement("div");
    leftDie.className = "diceWrapper-fixture";
    const rightDie = document.createElement("div");
    rightDie.className = "diceWrapper-fixture";
    rollGroup.append(leftDie, rightDie);
    const pass = document.createElement("button");
    pass.id = "action-button-pass-turn";
    const dieClicks = vi.fn();
    const passClicks = vi.fn();
    leftDie.addEventListener("click", dieClicks);
    pass.addEventListener("click", passClicks);
    document.body.append(rollGroup, pass);

    expect(visibleTurnControl()).toBe("roll");
    renderActionGuide(
      {
        kind: "turn-control",
        control: "roll",
        label: "Roll dice",
        signature: "exact-roll",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(dieClicks).toHaveBeenCalledOnce();
    expect(passClicks).not.toHaveBeenCalled();
  });

  it("treats Colonist's mounted inactive dice as already rolled", async () => {
    const rollGroup = document.createElement("div");
    rollGroup.id = "roll-dice-button";
    for (const value of ["five", "three"]) {
      const die = document.createElement("div");
      die.className = "diceWrapper-fixture";
      const image = document.createElement("img");
      image.className = "dice-fixture inactive-fixture";
      image.alt = value;
      die.append(image);
      rollGroup.append(die);
    }
    const pass = document.createElement("button");
    pass.id = "action-button-pass-turn";
    pass.textContent = "End turn";
    const passClicks = vi.fn();
    pass.addEventListener("click", passClicks);
    document.body.append(rollGroup, pass);

    expect(visibleTurnControl()).toBe("end");
    renderActionGuide(
      {
        kind: "turn-control",
        control: "end",
        label: "End turn",
        signature: "end-after-roll",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(passClicks).toHaveBeenCalledOnce();
  });

  it("clicks the exact inner road control instead of its multi-build wrapper", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "roadButton-fixture";
    const road = document.createElement("div");
    road.className = "root-fixture actionButton-fixture";
    const roadImage = document.createElement("img");
    roadImage.src = "https://cdn.colonist.io/dist/assets/road_red.fixture.svg";
    road.append(roadImage);
    const settlement = document.createElement("div");
    settlement.className = "root-fixture actionButton-fixture";
    const settlementImage = document.createElement("img");
    settlementImage.src =
      "https://cdn.colonist.io/dist/assets/settlement_red.fixture.svg";
    settlement.append(settlementImage);
    wrapper.append(road, settlement);
    const roadClicks = vi.fn();
    const settlementClicks = vi.fn();
    road.addEventListener("click", roadClicks);
    settlement.addEventListener("click", settlementClicks);
    document.body.append(wrapper);

    renderActionGuide(
      {
        kind: "build",
        build: "road",
        label: "Build road",
        signature: "exact-road-control",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(roadClicks).toHaveBeenCalledOnce();
    expect(settlementClicks).not.toHaveBeenCalled();
  });

  it.each([
    ["road", "action-button-build-road"],
    ["settlement", "action-button-build-settlement"],
  ] as const)(
    "uses Colonist's exact %s control even when the current asset has no legacy filename",
    async (build, id) => {
      const control = document.createElement("div");
      control.id = id;
      control.className = "currentActionControl-fixture";
      control.innerHTML = '<img src="https://cdn.colonist.io/current-piece.svg">';
      const clicked = vi.fn();
      control.addEventListener("click", clicked);
      document.body.append(control);

      renderActionGuide(
        {
          kind: "build",
          build,
          label: `Build ${build}`,
          signature: `current-${build}-control`,
          confidence: 1,
        },
        { highlight: true, autonomous: true },
      );
      await vi.advanceTimersByTimeAsync(800);

      expect(clicked).toHaveBeenCalledOnce();
    },
  );

  it("recovers when a recommended control mounts after the first render", async () => {
    const action = {
      kind: "build" as const,
      build: "road" as const,
      label: "Build road",
      signature: "late-road-control",
      confidence: 1,
    };
    const onExecution = vi.fn();

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      onExecution,
    });
    await vi.advanceTimersByTimeAsync(200);

    const control = document.createElement("div");
    control.id = "action-button-build-road";
    const clicked = vi.fn();
    control.addEventListener("click", clicked);
    document.body.append(control);
    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      onExecution,
    });
    await vi.advanceTimersByTimeAsync(800);

    expect(clicked).toHaveBeenCalledOnce();
    expect(onExecution).toHaveBeenCalledWith({
      succeeded: true,
      signature: "late-road-control",
    });
    expect(onExecution).not.toHaveBeenCalledWith(
      expect.objectContaining({ succeeded: false }),
    );
  });

  it("retries an ignored build-mode click while the same action remains current", async () => {
    const control = document.createElement("div");
    control.id = "action-button-build-road";
    const clicked = vi.fn();
    control.addEventListener("click", clicked);
    document.body.append(control);
    const action = {
      kind: "build" as const,
      build: "road" as const,
      label: "Build road",
      signature: "ignored-road-click",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => true,
    });
    await vi.advanceTimersByTimeAsync(1_850);

    expect(clicked).toHaveBeenCalledTimes(3);
    renderActionGuide(undefined, { highlight: true, autonomous: true });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(clicked).toHaveBeenCalledTimes(3);
  });

  it("releases an ignored build action after bounded retries so a fresh plan can retry", async () => {
    const control = document.createElement("div");
    control.id = "action-button-build-road";
    const clicked = vi.fn();
    control.addEventListener("click", clicked);
    document.body.append(control);
    const action = {
      kind: "build" as const,
      build: "road" as const,
      label: "Build road",
      signature: "ignored-road-recovery",
      confidence: 1,
    };
    const onExecution = vi.fn();
    const options = {
      highlight: true,
      autonomous: true,
      validate: () => true,
      onExecution,
    };

    renderActionGuide(action, options);
    await vi.advanceTimersByTimeAsync(5_400);

    expect(clicked).toHaveBeenCalledTimes(5);
    expect(onExecution).toHaveBeenCalledWith({
      succeeded: false,
      signature: "ignored-road-recovery",
      reason:
        "Colonist did not enter placement mode after bounded build-control retries",
    });

    // The failure callback causes the authoritative decision layer to replan.
    // Rendering that fresh result must not remain blocked behind the old
    // action signature or its exhausted retry counter.
    renderActionGuide(action, options);
    await vi.advanceTimersByTimeAsync(800);

    expect(clicked).toHaveBeenCalledTimes(6);
  });

  it("confirms a development card after opening Colonist's action panel", async () => {
    const card = document.createElement("div");
    card.className = "cardContainer-fixture";
    card.innerHTML = '<img src="card_knight.fixture.svg">';
    const cardClicks = vi.fn();
    const confirmClicks = vi.fn();
    card.addEventListener("click", () => {
      cardClicks();
      const modal = document.createElement("div");
      modal.className = "actionBox-fixture";
      const confirm = document.createElement("div");
      confirm.className = "confirmButton-fixture";
      confirm.innerHTML = '<img src="icon_check.fixture.svg">';
      confirm.addEventListener("click", confirmClicks);
      modal.append(confirm);
      document.body.append(modal);
    });
    document.body.append(card);

    renderActionGuide(
      {
        kind: "development",
        card: "knight",
        label: "Play knight",
        signature: "play-knight",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(1_200);

    expect(cardClicks).toHaveBeenCalledOnce();
    expect(confirmClicks).toHaveBeenCalledOnce();
  });

  it("validates a development workflow after the card leaves the playable inventory", async () => {
    let firstClickLegal = true;
    const card = document.createElement("div");
    card.className = "cardContainer-fixture";
    card.innerHTML = '<img src="card_yearofplenty.fixture.svg">';
    const resourceClicks = vi.fn();
    const confirmClicks = vi.fn();
    card.addEventListener("click", () => {
      firstClickLegal = false;
      const modal = document.createElement("div");
      modal.className = "actionBox-fixture";
      const confirm = document.createElement("div");
      confirm.className = "confirmButton-fixture";
      confirm.innerHTML = '<img src="icon_check.fixture.svg">';
      confirm.addEventListener("click", confirmClicks);
      const resource = document.createElement("button");
      resource.innerHTML = '<img src="card_brick.svg">';
      resource.addEventListener("click", resourceClicks);
      modal.append(confirm, resource);
      document.body.append(modal);
    });
    document.body.append(card);

    renderActionGuide(
      {
        kind: "development",
        card: "year-of-plenty",
        label: "Play year of plenty",
        signature: "play-yop",
        confidence: 1,
        followupResources: ["brick", "brick"],
      },
      {
        highlight: true,
        autonomous: true,
        validate: () => firstClickLegal,
        validateContinuation: () => true,
      },
    );
    await vi.advanceTimersByTimeAsync(2_000);

    expect(confirmClicks).toHaveBeenCalled();
    expect(resourceClicks).toHaveBeenCalledTimes(2);
  });

  it("keeps highlighting manual Year of Plenty picks in Colonist's action-box container", async () => {
    const card = document.createElement("div");
    card.className = "cardContainer-fixture";
    card.innerHTML = '<img src="card_yearofplenty.fixture.svg">';
    const resourceClicks = vi.fn();
    card.addEventListener("click", () => {
      const actionBox = document.createElement("div");
      actionBox.className = "actionBoxContainer-fixture";
      const confirmCard = document.createElement("button");
      confirmCard.className = "confirmButton-fixture";
      confirmCard.addEventListener("click", () => {
        const grain = document.createElement("div");
        grain.dataset.cardEnum = "4";
        grain.innerHTML = '<img src="card_grain.svg">';
        grain.addEventListener("click", resourceClicks);
        const confirmResources = document.createElement("button");
        confirmResources.className = "confirmButton-fixture";
        actionBox.replaceChildren(grain, confirmResources);
      });
      actionBox.append(confirmCard);
      document.body.append(actionBox);
    });
    document.body.append(card);

    renderActionGuide(
      {
        kind: "development",
        card: "year-of-plenty",
        label: "Play year of plenty",
        signature: "manual-yop",
        confidence: 1,
        followupResources: ["grain", "grain"],
      },
      { highlight: true, autonomous: false },
    );

    card.click();
    await vi.advanceTimersByTimeAsync(260);
    const confirmCard = document.querySelector<HTMLElement>(
      "[class*='confirmButton-']",
    )!;
    expect(
      document.querySelector("#colonist-assistant-action-guide span")
        ?.textContent,
    ).toContain("Confirm year of plenty");

    confirmCard.click();
    await vi.advanceTimersByTimeAsync(320);
    expect(
      document.querySelector("#colonist-assistant-action-guide span")
        ?.textContent,
    ).toContain("Choose grain 1/2");

    document.querySelector<HTMLElement>("[data-card-enum='4']")!.click();
    await vi.advanceTimersByTimeAsync(260);
    expect(resourceClicks).toHaveBeenCalledOnce();
    expect(
      document.querySelector("#colonist-assistant-action-guide span")
        ?.textContent,
    ).toContain("Choose grain 2/2");
  });

  it("sends a validated board command for canvas autopilot", async () => {
    const messages: unknown[] = [];
    window.addEventListener("message", (event) => {
      if (event.data?.source === "colonist-assistant-content") {
        messages.push(event.data);
      }
    });

    renderActionGuide(
      {
        kind: "board",
        boardAction: "settlement",
        targetId: "v:1,2,0",
        point: { x: 240, y: 180 },
        label: "Place settlement here",
        signature: "board-settlement",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(50);

    expect(messages).toContainEqual({
      source: "colonist-assistant-content",
      type: "execute-board-action",
      action: "settlement",
      targetId: "v:1,2,0",
      signature: "board-settlement",
      attempt: 1,
    });
    renderActionGuide(undefined, { highlight: true, autonomous: true });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(messages).toHaveLength(1);
  });

  it("releases a board placement after bounded validated commit retries", async () => {
    const attempts: number[] = [];
    const executions = vi.fn();
    window.addEventListener("message", (event) => {
      if (
        event.data?.source === "colonist-assistant-content" &&
        event.data?.type === "execute-board-action"
      ) {
        attempts.push(event.data.attempt);
      }
    });

    renderActionGuide(
      {
        kind: "board",
        boardAction: "settlement",
        targetId: "v:2,-1,0",
        point: { x: 320, y: 220 },
        label: "Place settlement here",
        signature: "ignored-board-settlement",
        confidence: 1,
      },
      {
        highlight: true,
        autonomous: true,
        validate: () => true,
        onExecution: executions,
      },
    );
    await vi.advanceTimersByTimeAsync(7_200);

    expect(attempts).toEqual([1, 2, 3, 4, 5]);
    expect(executions).toHaveBeenCalledWith({
      succeeded: false,
      signature: "ignored-board-settlement",
      reason:
        "Colonist did not commit board placement after bounded validated retries",
    });
  });

  it("cancels pending board retries when autopilot is disabled", async () => {
    const attempts: number[] = [];
    window.addEventListener("message", (event) => {
      if (
        event.data?.source === "colonist-assistant-content" &&
        event.data?.type === "execute-board-action"
      ) {
        attempts.push(event.data.attempt);
      }
    });
    const action = {
      kind: "board" as const,
      boardAction: "road" as const,
      targetId: "e:1,-1,0",
      point: { x: 360, y: 260 },
      label: "Build this road",
      signature: "disable-autopilot-board-road",
      confidence: 1,
    };
    const options = {
      highlight: true,
      autonomous: true,
      validate: () => true,
      validateBoardContinuation: () => true,
    };

    renderActionGuide(action, options);
    await vi.advanceTimersByTimeAsync(50);
    expect(attempts).toEqual([1]);

    renderActionGuide(action, {
      ...options,
      autonomous: false,
    });
    await vi.advanceTimersByTimeAsync(7_200);

    expect(attempts).toEqual([1]);
  });

  it("cancels pending board retries when a game transition destroys the guide", async () => {
    const attempts: number[] = [];
    window.addEventListener("message", (event) => {
      if (
        event.data?.source === "colonist-assistant-content" &&
        event.data?.type === "execute-board-action"
      ) {
        attempts.push(event.data.attempt);
      }
    });
    renderActionGuide(
      {
        kind: "board",
        boardAction: "settlement",
        targetId: "v:reused-base-map-target",
        point: { x: 360, y: 260 },
        label: "Build this settlement",
        signature: "old-game-settlement",
        confidence: 1,
      },
      {
        highlight: true,
        autonomous: true,
        validate: () => true,
        validateBoardContinuation: () => true,
      },
    );
    await vi.advanceTimersByTimeAsync(50);
    expect(attempts).toEqual([1]);

    destroyActionGuide();
    await vi.advanceTimersByTimeAsync(7_200);

    expect(attempts).toEqual([1]);
  });

  it("keeps a legal board placement retry alive while the overlay renders pending", async () => {
    const attempts: number[] = [];
    const executions = vi.fn();
    window.addEventListener("message", (event) => {
      if (
        event.data?.source === "colonist-assistant-content" &&
        event.data?.type === "execute-board-action"
      ) {
        attempts.push(event.data.attempt);
      }
    });
    const action = {
      kind: "board" as const,
      boardAction: "road" as const,
      targetId: "e:0,-1,0",
      point: { x: 340, y: 350 },
      label: "Build this road",
      signature: "ignored-board-road-rerender",
      confidence: 1,
    };
    const options = {
      highlight: true,
      autonomous: true,
      validate: () => true,
      validateBoardContinuation: () => true,
      onExecution: executions,
    };

    renderActionGuide(action, options);
    renderActionGuide(undefined, {
      ...options,
      validate: () => false,
    });
    await vi.advanceTimersByTimeAsync(7_200);

    expect(attempts).toEqual([1, 2, 3, 4, 5]);
    expect(executions).toHaveBeenCalledWith({
      succeeded: false,
      signature: "ignored-board-road-rerender",
      reason:
        "Colonist did not commit board placement after bounded validated retries",
    });
  });

  it("stops board retries once the placement phase advances", async () => {
    const attempts: number[] = [];
    const executions = vi.fn();
    let placementStillLegal = true;
    let expectedPlacementObserved = false;
    window.addEventListener("message", (event) => {
      if (
        event.data?.source === "colonist-assistant-content" &&
        event.data?.type === "execute-board-action"
      ) {
        attempts.push(event.data.attempt);
      }
    });
    const action = {
      kind: "board" as const,
      boardAction: "city" as const,
      targetId: "v:0,1,1",
      point: { x: 400, y: 300 },
      label: "Build this city",
      signature: "committed-board-city",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => placementStillLegal,
      validateBoardContinuation: () => placementStillLegal,
      validateBoardCommit: () => expectedPlacementObserved,
      onExecution: executions,
    });
    expect(executions).not.toHaveBeenCalled();
    placementStillLegal = false;
    expectedPlacementObserved = true;
    renderActionGuide(undefined, {
      highlight: true,
      autonomous: true,
    });
    await vi.advanceTimersByTimeAsync(3_000);

    expect(attempts).toEqual([1]);
    expect(executions).toHaveBeenCalledWith({
      succeeded: true,
      signature: "committed-board-city",
    });
  });

  it("reports a changed board without the requested placement as failure", () => {
    const executions = vi.fn();
    let placementStillLegal = true;
    const action = {
      kind: "board" as const,
      boardAction: "road" as const,
      targetId: "e:1,1,0",
      point: { x: 400, y: 300 },
      label: "Build this road",
      signature: "uncommitted-board-road",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => placementStillLegal,
      validateBoardContinuation: () => placementStillLegal,
      validateBoardCommit: () => false,
      onExecution: executions,
    });
    placementStillLegal = false;
    renderActionGuide(undefined, {
      highlight: true,
      autonomous: true,
    });

    expect(executions).toHaveBeenCalledWith({
      succeeded: false,
      signature: "uncommitted-board-road",
      reason:
        "Board state changed without the expected placement commit",
    });
  });

  it("cancels a stale robber-victim follow-up after the board phase advances", async () => {
    const playerModal = document.createElement("div");
    playerModal.className = "selectPlayer-fixture";
    const rival = document.createElement("button");
    rival.textContent = "Rival";
    const clicked = vi.fn();
    rival.addEventListener("click", clicked);
    playerModal.append(rival);

    renderActionGuide(
      {
        kind: "board",
        boardAction: "robber",
        targetId: "h:3,2",
        point: { x: 300, y: 240 },
        label: "Move robber here",
        signature: "robber-followup",
        confidence: 1,
        followupPlayer: "Rival",
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(60);
    renderActionGuide(undefined, { highlight: true, autonomous: true });
    document.body.append(playerModal);
    await vi.advanceTimersByTimeAsync(6_000);

    expect(clicked).not.toHaveBeenCalled();
  });

  it("releases the victim workflow as soon as Colonist closes the picker", async () => {
    const playerModal = document.createElement("div");
    playerModal.className = "selectPlayer-fixture";
    const rival = document.createElement("button");
    rival.textContent = "Rival";
    playerModal.append(rival);
    document.body.append(playerModal);

    renderActionGuide(
      {
        kind: "player",
        player: "Rival",
        label: "Steal from Rival",
        signature: "victim-workflow",
        confidence: 1,
      },
      { highlight: true, autonomous: false },
    );

    expect(activeWorkflowAction("none", true)?.kind).toBe("player");
    expect(activeWorkflowAction("none", false)).toBeUndefined();
  });

  it.each([
    ["decline", 1],
    ["accept", 2],
  ] as const)(
    "clicks the current Colonist %s trade control",
    async (verdict, expectedIndex) => {
      const wrapper = document.createElement("div");
      wrapper.className = "gameTradeOffersWrapper-fixture";
      const offer = document.createElement("div");
      offer.className = "tradeContainer-fixture";
      const clicks = [vi.fn(), vi.fn(), vi.fn()];
      for (const [index, clicked] of clicks.entries()) {
        const button = document.createElement("div");
        button.className = "tradeButton-fixture";
        button.addEventListener("click", clicked);
        offer.append(button);
      }
      wrapper.append(offer);
      document.body.append(wrapper);

      renderActionGuide(
        {
          kind: "trade",
          offerIndex: 0,
          tradeId: `offer-${verdict}`,
          verdict,
          label: `${verdict} trade`,
          signature: `trade-${verdict}`,
          confidence: 1,
        },
        { highlight: true, autonomous: true },
      );
      await vi.advanceTimersByTimeAsync(800);

      clicks.forEach((clicked, index) => {
        if (index === expectedIndex) expect(clicked).toHaveBeenCalledOnce();
        else expect(clicked).not.toHaveBeenCalled();
      });
    },
  );

  it("keeps a dispatched decline pending across a harmless guide-signature refresh", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const offer = document.createElement("div");
    offer.className = "tradeContainer-fixture";
    const clicks = [vi.fn(), vi.fn(), vi.fn()];
    for (const clicked of clicks) {
      const button = document.createElement("div");
      button.className = "tradeButton-fixture";
      button.addEventListener("click", clicked);
      offer.append(button);
    }
    wrapper.append(offer);
    document.body.append(wrapper);

    let rejectedVisibleInBridge = false;
    const executions = vi.fn();
    const action = {
      kind: "trade" as const,
      offerIndex: 0,
      tradeId: "live-offer",
      verdict: "decline" as const,
      label: "Decline trade",
      signature: "decline-before-refresh",
      confidence: 1,
    };

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      validate: () => true,
      validateControlCommit: () => rejectedVisibleInBridge,
      validateControlContinuation: () => true,
      onExecution: executions,
    });
    await vi.advanceTimersByTimeAsync(800);
    expect(clicks[1]).toHaveBeenCalledOnce();
    expect(executions).not.toHaveBeenCalled();

    renderActionGuide(
      { ...action, signature: "decline-after-refresh" },
      {
        highlight: true,
        autonomous: true,
        validate: () => true,
        validateControlCommit: () => rejectedVisibleInBridge,
        validateControlContinuation: () => true,
      },
    );
    await vi.advanceTimersByTimeAsync(140);

    rejectedVisibleInBridge = true;
    await vi.advanceTimersByTimeAsync(280);

    expect(executions).not.toHaveBeenCalledWith(
      expect.objectContaining({
        succeeded: false,
        reason: "Colonist state changed without the expected control commit",
      }),
    );
    expect(executions).toHaveBeenCalledWith({
      succeeded: true,
      signature: "decline-before-refresh",
    });
  });

  it("executes the enabled accepted-player check instead of an inert response", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const offer = document.createElement("div");
    offer.className = "tradeContainer-fixture";
    const clicks = [vi.fn(), vi.fn(), vi.fn(), vi.fn()];
    for (const [index, clicked] of clicks.entries()) {
      const button = document.createElement("div");
      button.className = "tradeButton-fixture";
      const foreground = document.createElement("div");
      if (index < 2) foreground.className = "foregroundDisabled-fixture";
      const icon = document.createElement("img");
      icon.src =
        index === 2
          ? "https://cdn.colonist.io/dist/assets/icon_check.fixture.svg"
          : "https://cdn.colonist.io/dist/assets/icon_x.fixture.svg";
      foreground.append(icon);
      button.append(foreground);
      button.addEventListener("click", clicked);
      offer.append(button);
    }
    wrapper.append(offer);
    document.body.append(wrapper);

    renderActionGuide(
      {
        kind: "trade-partner",
        offerIndex: 0,
        tradeId: "outgoing-offer",
        acceptedIndex: 0,
        player: "Ajax",
        label: "Trade with Ajax",
        signature: "execute-accepted-trade",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(clicks[2]).toHaveBeenCalledOnce();
    expect(clicks[0]).not.toHaveBeenCalled();
    expect(clicks[1]).not.toHaveBeenCalled();
    expect(clicks[3]).not.toHaveBeenCalled();
  });

  it("cancels a completed outgoing offer through the enabled cancel X instead of an inert response", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const offer = document.createElement("div");
    offer.className = "tradeContainer-fixture";
    const clicks = [vi.fn(), vi.fn(), vi.fn(), vi.fn()];
    for (const [index, clicked] of clicks.entries()) {
      const button = document.createElement("div");
      button.className = "tradeButton-fixture";
      const foreground = document.createElement("div");
      if (index < 2) foreground.className = "foregroundDisabled-fixture";
      const icon = document.createElement("img");
      icon.src =
        index === 2
          ? "https://cdn.colonist.io/dist/assets/icon_check.fixture.svg"
          : "https://cdn.colonist.io/dist/assets/icon_x.fixture.svg";
      foreground.append(icon);
      button.append(foreground);
      button.addEventListener("click", clicked);
      offer.append(button);
    }
    wrapper.append(offer);
    document.body.append(wrapper);

    renderActionGuide(
      {
        kind: "trade-cancel",
        offerIndex: 0,
        tradeId: "completed-outgoing-offer",
        label: "Cancel this accepted trade",
        signature: "cancel-completed-outgoing",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(clicks[3]).toHaveBeenCalledOnce();
    expect(clicks[0]).not.toHaveBeenCalled();
    expect(clicks[1]).not.toHaveBeenCalled();
    expect(clicks[2]).not.toHaveBeenCalled();
  });

  it("cancels an unanswered outgoing offer through its exact offer control", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const offer = document.createElement("div");
    offer.className = "tradeContainer-fixture";
    const cancel = document.createElement("button");
    cancel.className = "tradeButton-fixture";
    cancel.textContent = "Cancel offer";
    const cancelClick = vi.fn();
    cancel.addEventListener("click", cancelClick);
    offer.append(cancel);
    wrapper.append(offer);
    document.body.append(wrapper);

    renderActionGuide(
      {
        kind: "trade-cancel",
        offerIndex: 0,
        tradeId: "outgoing-offer",
        label: "Cancel unanswered trade",
        signature: "cancel-outgoing",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(800);

    expect(cancelClick).toHaveBeenCalledOnce();
  });

  it("opens and completes a recommended counteroffer", async () => {
    const wrapper = document.createElement("div");
    wrapper.className = "gameTradeOffersWrapper-fixture";
    const offer = document.createElement("div");
    offer.className = "tradeContainer-fixture";
    const counter = document.createElement("button");
    counter.className = "tradeButton-fixture";
    const decline = document.createElement("button");
    decline.className = "tradeButton-fixture";
    const accept = document.createElement("button");
    accept.className = "tradeButton-fixture";
    const clicks: string[] = [];
    counter.addEventListener("click", () => {
      clicks.push("counter");
      const inventory = document.createElement("div");
      inventory.id = "player-card-inventory";
      inventory.innerHTML =
        '<button class="card"><img src="card_brick.svg"></button>';
      const offeredProposal = document.createElement("div");
      offeredProposal.className = "proposalOfferedHalfContainer-fixture";
      const wantedProposal = document.createElement("div");
      wantedProposal.className = "proposalWantedHalfContainer-fixture";
      inventory.querySelector("button")?.addEventListener("click", () => {
        clicks.push("give-brick");
        offeredProposal.innerHTML =
          '<button data-card-enum="2"><img src="card_brick.svg"></button>';
      });
      const wanted = document.createElement("div");
      wanted.className = "wantedCardSelectorContainer-fixture";
      wanted.innerHTML =
        '<button class="card"><img src="card_lumber.svg"></button>';
      wanted.querySelector("button")?.addEventListener("click", () => {
        clicks.push("get-lumber");
        wantedProposal.innerHTML =
          '<button data-card-enum="1"><img src="card_lumber.svg"></button>';
      });
      const send = document.createElement("button");
      send.id = "action-button-trade-players";
      send.addEventListener("click", () => {
        clicks.push("send");
        inventory.remove();
        wanted.remove();
        offeredProposal.remove();
        wantedProposal.remove();
        send.remove();
      });
      document.body.append(
        inventory,
        wanted,
        offeredProposal,
        wantedProposal,
        send,
      );
    });
    offer.append(counter, decline, accept);
    wrapper.append(offer);
    document.body.append(wrapper);
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.lumber = 1;

    renderActionGuide(
      {
        kind: "trade",
        offerIndex: 0,
        tradeId: "incoming-counter",
        verdict: "counter",
        counterGive: give,
        counterReceive: receive,
        label: "Counter trade",
        signature: "trade-counter",
        confidence: 0.8,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(3_500);

    expect(clicks).toEqual([
      "counter",
      "give-brick",
      "get-lumber",
      "send",
    ]);
  });

  it("completes every click in an outgoing player trade", async () => {
    const open = document.createElement("button");
    open.id = "action-button-trade";
    const clicks: string[] = [];
    open.addEventListener("click", () => {
      clicks.push("open");
      const inventory = document.createElement("div");
      inventory.id = "player-card-inventory";
      inventory.innerHTML =
        '<button class="card"><img src="card_brick.svg"></button>';
      const offeredProposal = document.createElement("div");
      offeredProposal.className = "proposalOfferedHalfContainer-fixture";
      const wantedProposal = document.createElement("div");
      wantedProposal.className = "proposalWantedHalfContainer-fixture";
      inventory.querySelector("button")?.addEventListener("click", () => {
        clicks.push("give-brick");
        offeredProposal.innerHTML =
          '<button data-card-enum="2"><img src="card_brick.svg"></button>';
      });
      const wanted = document.createElement("div");
      wanted.className = "wantedCardSelectorContainer-fixture";
      wanted.innerHTML =
        '<button class="card"><img src="card_lumber.svg"></button>';
      wanted.querySelector("button")?.addEventListener("click", () => {
        clicks.push("get-lumber");
        wantedProposal.innerHTML =
          '<button data-card-enum="1"><img src="card_lumber.svg"></button>';
      });
      const send = document.createElement("button");
      send.id = "action-button-trade-players";
      send.addEventListener("click", () => {
        clicks.push("send");
        inventory.remove();
        wanted.remove();
        offeredProposal.remove();
        wantedProposal.remove();
        send.remove();
      });
      document.body.append(
        inventory,
        wanted,
        offeredProposal,
        wantedProposal,
        send,
      );
    });
    document.body.append(open);
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.lumber = 1;

    const action = {
      kind: "trade-builder" as const,
      mode: "player" as const,
      give,
      receive,
      label: "Make trade",
      signature: "trade",
      confidence: 0.7,
    };
    const originatingExecution = vi.fn();
    const laterRenderExecution = vi.fn();
    let postSendRefreshes = 0;
    window.addEventListener("colonist-assistant-board-refresh", () => {
      if (
        clicks.includes("send") &&
        !document.querySelector("#action-button-trade-players")
      ) {
        postSendRefreshes += 1;
      }
    });
    renderActionGuide(action, {
      highlight: true,
      autonomous: false,
      onExecution: originatingExecution,
    });
    await vi.advanceTimersByTimeAsync(50);
    expect(clicks).toEqual([]);

    renderActionGuide(action, {
      highlight: true,
      autonomous: true,
      onExecution: laterRenderExecution,
    });
    await vi.advanceTimersByTimeAsync(3_500);

    expect(clicks).toEqual([
      "open",
      "give-brick",
      "get-lumber",
      "send",
    ]);
    expect(originatingExecution).toHaveBeenCalledWith({
      succeeded: true,
      signature: "trade",
    });
    expect(laterRenderExecution).not.toHaveBeenCalled();
    expect(postSendRefreshes).toBeGreaterThanOrEqual(2);
  });

  it("retries an idempotent trade-panel control when Colonist drops the first click", async () => {
    const open = document.createElement("button");
    open.id = "action-button-trade";
    let openAttempts = 0;
    const clicks: string[] = [];
    open.addEventListener("click", () => {
      openAttempts += 1;
      if (openAttempts === 1) return;
      clicks.push("open");
      const inventory = document.createElement("div");
      inventory.id = "player-card-inventory";
      inventory.innerHTML =
        '<button class="card"><img src="card_brick.svg"></button>';
      const offeredProposal = document.createElement("div");
      offeredProposal.className = "proposalOfferedHalfContainer-fixture";
      const wantedProposal = document.createElement("div");
      wantedProposal.className = "proposalWantedHalfContainer-fixture";
      inventory.querySelector("button")?.addEventListener("click", () => {
        clicks.push("give-brick");
        offeredProposal.innerHTML =
          '<button data-card-enum="2"><img src="card_brick.svg"></button>';
      });
      const wanted = document.createElement("div");
      wanted.className = "wantedCardSelectorContainer-fixture";
      wanted.innerHTML =
        '<button class="card"><img src="card_lumber.svg"></button>';
      wanted.querySelector("button")?.addEventListener("click", () => {
        clicks.push("get-lumber");
        wantedProposal.innerHTML =
          '<button data-card-enum="1"><img src="card_lumber.svg"></button>';
      });
      const send = document.createElement("button");
      send.id = "action-button-trade-players";
      send.addEventListener("click", () => {
        clicks.push("send");
        inventory.remove();
        wanted.remove();
        offeredProposal.remove();
        wantedProposal.remove();
        send.remove();
      });
      document.body.append(
        inventory,
        wanted,
        offeredProposal,
        wantedProposal,
        send,
      );
    });
    document.body.append(open);
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.lumber = 1;
    const executions = vi.fn();

    renderActionGuide(
      {
        kind: "trade-builder",
        mode: "player",
        give,
        receive,
        label: "Make trade",
        signature: "dropped-open-click",
        confidence: 0.9,
      },
      {
        highlight: true,
        autonomous: true,
        onExecution: executions,
      },
    );
    await vi.advanceTimersByTimeAsync(5_000);

    expect(openAttempts).toBe(2);
    expect(clicks).toEqual([
      "open",
      "give-brick",
      "get-lumber",
      "send",
    ]);
    expect(executions).toHaveBeenCalledWith({
      succeeded: true,
      signature: "dropped-open-click",
    });
  });

  it("aborts, closes, and reports a player trade rejected by Colonist", async () => {
    const open = document.createElement("button");
    open.id = "action-button-trade";
    const sendClick = vi.fn();
    const executions = vi.fn();
    const closePanel = () => {
      document.querySelector("#player-card-inventory")?.remove();
      document
        .querySelector("[class*='wantedCardSelectorContainer-']")
        ?.remove();
      document
        .querySelector("[class*='proposalOfferedHalfContainer-']")
        ?.remove();
      document
        .querySelector("[class*='proposalWantedHalfContainer-']")
        ?.remove();
      document.querySelector("#action-button-trade-players")?.remove();
    };
    open.addEventListener("click", () => {
      if (document.querySelector("#player-card-inventory")) {
        closePanel();
        return;
      }
      const inventory = document.createElement("div");
      inventory.id = "player-card-inventory";
      inventory.innerHTML =
        '<button><img src="card_brick.svg"></button>';
      const wanted = document.createElement("div");
      wanted.className = "wantedCardSelectorContainer-fixture";
      wanted.innerHTML =
        '<button><img src="card_lumber.svg"></button>';
      const offeredProposal = document.createElement("div");
      offeredProposal.className = "proposalOfferedHalfContainer-fixture";
      const wantedProposal = document.createElement("div");
      wantedProposal.className = "proposalWantedHalfContainer-fixture";
      inventory.querySelector("button")?.addEventListener("click", () => {
        offeredProposal.innerHTML =
          '<button data-card-enum="2"><img src="card_brick.svg"></button>';
      });
      wanted.querySelector("button")?.addEventListener("click", () => {
        wantedProposal.innerHTML =
          '<button data-card-enum="1"><img src="card_lumber.svg"></button>';
        const log = document.createElement("div");
        log.id = "game-log-text";
        const message = document.createElement("div");
        message.dataset.index = "315";
        message.textContent = "Players do not have enough resources";
        log.append(message);
        document.body.append(log);
      });
      const send = document.createElement("button");
      send.id = "action-button-trade-players";
      send.addEventListener("click", sendClick);
      document.body.append(
        inventory,
        wanted,
        offeredProposal,
        wantedProposal,
        send,
      );
    });
    document.body.append(open);
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.lumber = 1;

    renderActionGuide(
      {
        kind: "trade-builder",
        mode: "player",
        give,
        receive,
        label: "Make trade",
        signature: "rejected-trade",
        confidence: 0.8,
      },
      {
        highlight: true,
        autonomous: true,
        onExecution: executions,
      },
    );
    await vi.advanceTimersByTimeAsync(4_000);

    expect(sendClick).not.toHaveBeenCalled();
    expect(executions).toHaveBeenCalledWith(
      expect.objectContaining({
        succeeded: false,
        signature: "rejected-trade",
        reason: expect.stringContaining(
          "Players do not have enough resources",
        ),
      }),
    );
    expect(document.querySelector("#player-card-inventory")).toBeNull();
  });

  it("closes a stale trade panel before playing a development card", async () => {
    const trade = document.createElement("button");
    trade.id = "action-button-trade";
    const inventory = document.createElement("div");
    inventory.id = "player-card-inventory";
    const wanted = document.createElement("div");
    wanted.className = "wantedCardSelectorContainer-fixture";
    const proposal = document.createElement("div");
    proposal.className = "proposalWantedHalfContainer-fixture";
    const submit = document.createElement("button");
    submit.id = "action-button-trade-players";
    const closeClick = vi.fn();
    trade.addEventListener("click", () => {
      closeClick();
      inventory.remove();
      wanted.remove();
      proposal.remove();
      submit.remove();
    });
    const card = document.createElement("button");
    card.className = "cardContainer-fixture";
    card.innerHTML = '<img src="card_knight.svg">';
    const cardClick = vi.fn();
    const confirmClick = vi.fn();
    card.addEventListener("click", () => {
      cardClick();
      const modal = document.createElement("div");
      modal.className = "actionBox-fixture";
      const confirm = document.createElement("button");
      confirm.className = "confirmButton-fixture";
      confirm.addEventListener("click", confirmClick);
      modal.append(confirm);
      document.body.append(modal);
    });
    document.body.append(trade, inventory, wanted, proposal, submit, card);
    const action = {
      kind: "development" as const,
      card: "knight" as const,
      label: "Play knight",
      signature: "close-then-knight",
      confidence: 1,
    };
    const options = { highlight: true, autonomous: true };

    renderActionGuide(action, options);
    await vi.advanceTimersByTimeAsync(700);
    expect(closeClick).toHaveBeenCalledOnce();
    expect(cardClick).not.toHaveBeenCalled();

    renderActionGuide(action, options);
    await vi.advanceTimersByTimeAsync(1_400);
    expect(cardClick).toHaveBeenCalledOnce();
    expect(confirmClick).toHaveBeenCalledOnce();
  });

  it("does not mistake Colonist's persistent hand selectors for an open trade panel", async () => {
    const trade = document.createElement("button");
    trade.id = "action-button-trade";
    const tradeClick = vi.fn();
    trade.addEventListener("click", tradeClick);
    const inventory = document.createElement("div");
    inventory.id = "player-card-inventory";
    const wanted = document.createElement("div");
    wanted.className = "wantedCardSelectorContainer-fixture";
    const card = document.createElement("button");
    card.className = "cardContainer-fixture";
    card.innerHTML = '<img src="card_knight.svg">';
    const cardClick = vi.fn();
    card.addEventListener("click", () => {
      cardClick();
      const modal = document.createElement("div");
      modal.className = "actionBox-fixture";
      const confirm = document.createElement("button");
      confirm.className = "confirmButton-fixture";
      modal.append(confirm);
      document.body.append(modal);
    });
    document.body.append(trade, inventory, wanted, card);

    renderActionGuide(
      {
        kind: "development",
        card: "knight",
        label: "Play knight",
        signature: "persistent-hand-knight",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(1_200);

    expect(cardClick).toHaveBeenCalledOnce();
    expect(tradeClick).not.toHaveBeenCalled();
  });

  it("cancels a multi-click trade as soon as its state validation expires", async () => {
    let valid = true;
    const open = document.createElement("button");
    open.id = "action-button-trade";
    const resourceClick = vi.fn();
    const sendClick = vi.fn();
    open.addEventListener("click", () => {
      valid = false;
      const inventory = document.createElement("div");
      inventory.id = "player-card-inventory";
      const brick = document.createElement("button");
      brick.className = "card";
      brick.innerHTML = '<img src="card_brick.svg">';
      brick.addEventListener("click", resourceClick);
      inventory.append(brick);
      const wanted = document.createElement("div");
      wanted.className = "wantedCardSelectorContainer-fixture";
      wanted.innerHTML =
        '<button class="card"><img src="card_lumber.svg"></button>';
      const send = document.createElement("button");
      send.id = "action-button-trade-players";
      send.addEventListener("click", sendClick);
      document.body.append(inventory, wanted, send);
    });
    document.body.append(open);
    const give = emptyResources();
    give.brick = 1;
    const receive = emptyResources();
    receive.lumber = 1;

    renderActionGuide(
      {
        kind: "trade-builder",
        mode: "player",
        give,
        receive,
        label: "Make trade",
        signature: "stale-trade",
        confidence: 0.9,
      },
      {
        highlight: true,
        autonomous: true,
        validate: () => valid,
      },
    );
    await vi.advanceTimersByTimeAsync(2_000);

    expect(resourceClick).not.toHaveBeenCalled();
    expect(sendClick).not.toHaveBeenCalled();
  });

  it("retries a swallowed repeated discard before confirming", async () => {
    const modal = document.createElement("div");
    modal.className = "actionBox-fixture";
    modal.append("Selecciona cartas");
    const inventory = document.createElement("div");
    inventory.id = "player-card-inventory";
    const brick = document.createElement("button");
    brick.className = "card";
    brick.innerHTML = '<img src="card_brick.svg">';
    inventory.append(brick);
    const progress = document.createElement("span");
    progress.textContent = "0/2";
    const confirm = document.createElement("div");
    confirm.className = "confirmButton-fixture";
    confirm.innerHTML = '<img src="icon_check.fixture.svg">';
    const brickClicks = vi.fn();
    const confirmClicks = vi.fn();
    let selectedCount = 0;
    brick.addEventListener("click", () => {
      brickClicks();
      // Colonist can swallow a click while keeping one collapsed selected-card
      // stack visible. The total selected counter is the independent commit
      // evidence for the repeated copy.
      if (brickClicks.mock.calls.length === 2) return;
      selectedCount = Math.min(2, selectedCount + 1);
      progress.textContent = `${selectedCount}/2`;
      if (!modal.querySelector("[data-card-enum='2']")) {
        const selected = document.createElement("button");
        selected.dataset.cardEnum = "2";
        selected.innerHTML = '<img src="card_brick.svg">';
        modal.prepend(selected);
      }
    });
    confirm.addEventListener("click", () => {
      confirmClicks();
      modal.remove();
    });
    modal.append(progress, confirm);
    document.body.append(modal, inventory);
    const cards = emptyResources();
    cards.brick = 2;

    renderActionGuide(
      {
        kind: "discard",
        cards,
        label: "Discard 2 cards",
        signature: "discard",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(3_000);

    expect(brickClicks).toHaveBeenCalledTimes(3);
    expect(confirmClicks).toHaveBeenCalledOnce();

    // The board bridge can briefly retain the completed mandatory phase while
    // Colonist commits the transaction. Re-rendering that identical snapshot
    // must not start a second discard workflow against the reduced hand.
    renderActionGuide(
      {
        kind: "discard",
        cards,
        label: "Discard 2 cards",
        signature: "discard",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(1_000);

    expect(brickClicks).toHaveBeenCalledTimes(3);
    expect(confirmClicks).toHaveBeenCalledOnce();
  });

  it("does not use total discard progress as proof of a repeated resource", async () => {
    const modal = document.createElement("div");
    modal.className = "actionBox-fixture";
    modal.append("Selecciona cartas");
    const progress = document.createElement("span");
    progress.textContent = "3/3";
    const selectedBrick = document.createElement("button");
    selectedBrick.dataset.cardEnum = "2";
    selectedBrick.innerHTML = '<img src="card_brick.svg">';
    const selectedWool = document.createElement("button");
    selectedWool.dataset.cardEnum = "3";
    selectedWool.innerHTML = '<img src="card_wool.svg">';
    const confirm = document.createElement("div");
    confirm.className = "confirmButton-fixture";
    confirm.innerHTML = '<img src="icon_check.fixture.svg">';
    const inventory = document.createElement("div");
    inventory.id = "player-card-inventory";
    const brick = document.createElement("button");
    brick.innerHTML = '<img src="card_brick.svg">';
    const wool = document.createElement("button");
    wool.innerHTML = '<img src="card_wool.svg">';
    inventory.append(brick, wool);
    const brickClicks = vi.fn();
    const confirmClicks = vi.fn();
    brick.addEventListener("click", () => {
      brickClicks();
      const secondBrick = document.createElement("button");
      secondBrick.dataset.cardEnum = "2";
      secondBrick.innerHTML = '<img src="card_brick.svg">';
      modal.prepend(secondBrick);
    });
    confirm.addEventListener("click", () => {
      confirmClicks();
      modal.remove();
    });
    modal.append(selectedBrick, selectedWool, progress, confirm);
    document.body.append(modal, inventory);
    const cards = emptyResources();
    cards.brick = 2;
    cards.wool = 1;

    renderActionGuide(
      {
        kind: "discard",
        cards,
        label: "Discard 3 cards",
        signature: "discard-preselected-mismatch",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );
    await vi.advanceTimersByTimeAsync(2_000);

    expect(brickClicks).toHaveBeenCalledOnce();
    expect(confirmClicks).toHaveBeenCalledOnce();
  });

  it("releases a mandatory workflow as soon as Colonist advances phases", () => {
    const cards = emptyResources();
    cards.grain = 3;
    renderActionGuide(
      {
        kind: "discard",
        cards,
        label: "Discard 3 cards",
        signature: "discard-phase",
        confidence: 1,
      },
      { highlight: true, autonomous: true },
    );

    expect(activeWorkflowAction("discard")?.kind).toBe("discard");
    expect(activeWorkflowAction("none")).toBeUndefined();
    expect(
      document.getElementById("colonist-assistant-action-guide"),
    ).toBeNull();
  });
});
