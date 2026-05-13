const BASE = "/api";

export interface Ap {
	bssid:        string;
	ssid:         string | null;
	channel:      number;
	rssi:         number;
	mfpr:         boolean;
	mfpc:         boolean;
	first_seen:   number;
	last_seen:    number;
	device_count: number;
}

export interface Device {
	mac:           string;
	last_bssid:    string;
	is_randomized: boolean;
	first_seen:    number;
	last_seen:     number;
}

export interface TotalPoint {
	timestamp_sec: number;
	total:         number;
}

export interface DevicePoint {
	timestamp_sec: number;
	mac:           string;
	mgmt:          number;
	ctrl:          number;
	data:          number;
	total:         number;
}

export interface Event {
	mac:        string;
	bssid:      string;
	event_type: "assoc" | "reassoc" | "disassoc" | "deauth";
	timestamp:  number;
}

export interface EventCounts {
	mac:         string;
	connects:    number;
	disconnects: number;
}

export async function fetchAps(): Promise<Ap[]> {
	const r = await fetch(`${BASE}/aps`);
	return r.json();
}

export async function fetchDevices(): Promise<Device[]> {
	const r = await fetch(`${BASE}/devices`);
	return r.json();
}

export async function fetchTotalSeries(fromUs?: number, toUs?: number): Promise<TotalPoint[]> {
	const params = new URLSearchParams();
	if (fromUs != null) params.set("from_us", String(fromUs));
	if (toUs   != null) params.set("to_us",   String(toUs));
	const r = await fetch(`${BASE}/timeseries/total?${params}`);
	return r.json();
}

export async function fetchDeviceSeries(fromUs?: number, toUs?: number): Promise<DevicePoint[]> {
	const params = new URLSearchParams();
	if (fromUs != null) params.set("from_us", String(fromUs));
	if (toUs   != null) params.set("to_us",   String(toUs));
	const r = await fetch(`${BASE}/timeseries/by-device?${params}`);
	return r.json();
}

export async function fetchEvents(): Promise<Event[]> {
	const r = await fetch(`${BASE}/events`);
	return r.json();
}

export async function fetchEventCounts(): Promise<EventCounts[]> {
	const r = await fetch(`${BASE}/events/counts`);
	return r.json();
}

export function formatMac(mac: string): string {
	return mac.toUpperCase();
}

export function formatTs(us: number): string {
	return new Date(us / 1000).toLocaleTimeString();
}
