// The guest page (0.10; the owner, 2026-10-30): its own entry, apart from the app's shell, which asks for a sign-in;
// this one only ever calls /api/guest/*, which the server answers on the home network alone.
import { mount } from 'svelte';
import '../app.css';
import Guest from './Guest.svelte';

export default mount(Guest, { target: document.getElementById('guest')! });
