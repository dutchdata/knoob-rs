"use client";
import { useEffect, useRef, useState, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
	fetchDeviceSeries, fetchEventCounts, fetchDevices,
	type DevicePoint, type EventCounts, type Device,
	formatMac, formatTs,
} from "@/lib/api";
import { usePoll } from "@/lib/usePoll";

function DeviceChart({ points, mac }: { points: DevicePoint[]; mac: string }) {
	const ref = useRef<HTMLDivElement>(null);
	const plotRef = useRef<uPlot | null>(null);

	// destroy + recreate when mac changes so axes/state reset
	useEffect(() => {
		return () => {
			plotRef.current?.destroy();
			plotRef.current = null;
		};
	}, [mac]);

	useEffect(() => {
		if (!ref.current || points.length === 0) return;
		const ts   = points.map(p => p.timestamp_sec);
		const mgmt = points.map(p => p.mgmt);
		const ctrl = points.map(p => p.ctrl);
		const data = points.map(p => p.data);
		const axisOpts = {
			stroke: "#6b6b8a",
			ticks:  { stroke: "#2a2a3a" },
			grid:   { stroke: "#2a2a3a" },
		};
		const opts: uPlot.Options = {
			width:  ref.current.clientWidth,
			height: 220,
			series: [
				{},
				{ label: "mgmt", stroke: "#7c6af7", width: 2 },
				{ label: "ctrl", stroke: "#3ecfcf", width: 2 },
				{ label: "data", stroke: "#3ecf8e", width: 2 },
			],
			axes: [
				{ ...axisOpts, values: (_u, vals) => vals.map(v => new Date(v * 1000).toLocaleTimeString()) },
				axisOpts,
			],
		};
		if (plotRef.current) {
			plotRef.current.setData([ts, mgmt, ctrl, data]);
		} else {
			plotRef.current = new uPlot(opts, [ts, mgmt, ctrl, data], ref.current);
		}
	}, [points]);

	useEffect(() => {
		const obs = new ResizeObserver(() => {
			if (ref.current && plotRef.current) {
				plotRef.current.setSize({ width: ref.current.clientWidth, height: 220 });
			}
		});
		if (ref.current) obs.observe(ref.current);
		return () => obs.disconnect();
	}, []);

	return <div ref={ref} style={{ width: "100%" }} />;
}

function EventBar({ label, connects, disconnects }: { label: string; connects: number; disconnects: number }) {
	const max = Math.max(connects, disconnects, 1);
	return (
		<div style={{ display: "grid", gridTemplateColumns: "120px 1fr 1fr", gap: 8, alignItems: "center", padding: "4px 0" }}>
			<span style={{ color: "var(--muted)", fontSize: 11, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
			<div style={{ display: "flex", alignItems: "center", gap: 4 }}>
				<div style={{ height: 12, width: `${(connects / max) * 100}%`, background: "var(--success)", borderRadius: 2, minWidth: connects > 0 ? 2 : 0 }} />
				<span style={{ color: "var(--success)", fontSize: 11 }}>{connects}</span>
			</div>
			<div style={{ display: "flex", alignItems: "center", gap: 4 }}>
				<div style={{ height: 12, width: `${(disconnects / max) * 100}%`, background: "var(--danger)", borderRadius: 2, minWidth: disconnects > 0 ? 2 : 0 }} />
				<span style={{ color: "var(--danger)", fontSize: 11 }}>{disconnects}</span>
			</div>
		</div>
	);
}

function DeviceDetail() {
	const params = useSearchParams();
	const mac    = params.get("mac") ?? "";
	const [allPoints,   setAllPoints]   = useState<DevicePoint[]>([]);
	const [eventCounts, setEventCounts] = useState<EventCounts[]>([]);
	const [device,      setDevice]      = useState<Device | null>(null);
	const [pollMs,      setPollMs]      = useState(3000);
	const [lastUpdate,  setLastUpdate]  = useState("");

	async function refresh() {
		const [series, ec, devices] = await Promise.all([
			fetchDeviceSeries(),
			fetchEventCounts(),
			fetchDevices(),
		]);
		setAllPoints(series.filter(p => p.mac === mac));
		setEventCounts(ec);
		setDevice(devices.find(d => d.mac === mac) ?? null);
		setLastUpdate(new Date().toLocaleTimeString());
	}

	usePoll(refresh, pollMs);

	const myEc = eventCounts.find(e => e.mac === mac);

	return (
		<div style={{ maxWidth: 1100, margin: "0 auto", padding: "24px 16px" }}>
			<div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 24 }}>
				<div style={{ display: "flex", alignItems: "center", gap: 12 }}>
					<Link href="/" style={{ color: "var(--muted)", fontSize: 18 }}>←</Link>
					<h1 style={{ fontSize: 16, color: "var(--accent)", letterSpacing: 1 }}>
						{mac ? formatMac(mac) : "unknown device"}
					</h1>
		{device?.is_randomized && <span style={{ color: "var(--accent)", fontSize: 10, border: "1px solid var(--accent)", padding: "1px 5px", borderRadius: 3 }}>RANDOMIZED</span>}
	</div>
	<div style={{ display: "flex", alignItems: "center", gap: 16 }}>
		<label style={{ color: "var(--muted)", display: "flex", alignItems: "center", gap: 8 }}>
			poll
			<input
				type="number"
				min={500}
				step={500}
				value={pollMs}
				onChange={e => setPollMs(Math.max(500, Number(e.target.value)))}
				style={{
					width: 70, background: "var(--surface)", border: "1px solid var(--border)",
						color: "var(--text)", padding: "2px 6px", borderRadius: 4,
				}}
			/>
			ms
		</label>
		<span style={{ color: "var(--muted)" }}>updated {lastUpdate}</span>
	</div>
</div>
{device && (
	<section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 20, display: "flex", gap: 32 }}>
		<div><div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>associated AP</div><div>{formatMac(device.last_bssid)}</div></div>
		<div><div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>first seen</div><div>{formatTs(device.first_seen)}</div></div>
		<div><div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>last seen</div><div>{formatTs(device.last_seen)}</div></div>
		{myEc && <>
		<div><div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>connects</div><div style={{ color: "var(--success)" }}>{myEc.connects}</div></div>
		<div><div style={{ color: "var(--muted)", fontSize: 10, textTransform: "uppercase", marginBottom: 4 }}>disconnects</div><div style={{ color: "var(--danger)" }}>{myEc.disconnects}</div></div>
</>}
	</section>
      )}
      <section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 20 }}>
	      <h2 style={{ color: "var(--muted)", fontSize: 11, letterSpacing: 2, marginBottom: 12, textTransform: "uppercase" }}>frames / sec by type</h2>
		{allPoints.length > 0
			? <DeviceChart points={allPoints} mac={mac} />
			: <div style={{ height: 220, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>waiting for data…</div>
		}
	</section>
	<section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16 }}>
		<h2 style={{ color: "var(--muted)", fontSize: 11, letterSpacing: 2, marginBottom: 4, textTransform: "uppercase" }}>connects / disconnects</h2>
		<div style={{ display: "grid", gridTemplateColumns: "120px 1fr 1fr", gap: 8, marginBottom: 8 }}>
			<span />
			<span style={{ color: "var(--success)", fontSize: 10, textTransform: "uppercase" }}>connects</span>
			<span style={{ color: "var(--danger)",  fontSize: 10, textTransform: "uppercase" }}>disconnects</span>
		</div>
		{eventCounts.length === 0
			? <div style={{ color: "var(--muted)" }}>no events</div>
			: eventCounts.map(ec => (
				<EventBar
					key={ec.mac}
					label={formatMac(ec.mac)}
					connects={ec.connects}
					disconnects={ec.disconnects}
				/>
				))
		}
	</section>
</div>
  );
}

export default function DevicePage() {
	return (
		<Suspense fallback={<div style={{ padding: 24, color: "var(--muted)" }}>loading…</div>}>
			<DeviceDetail />
		</Suspense>
	);
}
