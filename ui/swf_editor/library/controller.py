"""Bounded, asynchronous library state independent of ImGui and GL."""
from __future__ import annotations

from collections import OrderedDict, deque
from concurrent.futures import ThreadPoolExecutor, CancelledError, Future
import multiprocessing as mp
from pathlib import Path
import queue
from threading import Event, Thread
import time

from ui.swf_editor.library.catalog import Asset, Catalog, Cancelled, Filters, PAGE_SIZE, add_personal

MAX_PAGES = 8
MAX_PREVIEWS = 6
MAX_PIXELS_BYTES = 16 * 1024 * 1024


class LibraryController:
    def __init__(self, base: Path | None, personal: Path, cache_dir: Path, game: str):
        self.catalog = Catalog(base, personal, cache_dir / 'catalogs', game)
        self.cache_dir = cache_dir
        self.filters = Filters()
        self.pages = OrderedDict()
        self.total = 0
        self.categories, self.sources = [], []
        self.error, self.status = '', 'Loading library metadata…'
        self.selection: Asset | None = None
        self.geometry, self.shape, self.selection_error = None, None, ''
        self.pixels, self.errors, self.warnings = OrderedDict(), OrderedDict(), OrderedDict()
        self.import_status = ''
        self.metrics = dict(previews=0, disk_hits=0, stale_results=0)
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix='swf-library-catalog')
        self._future = None
        self._cancel = Event()
        self._generation = 0
        self._page_requests = deque()
        self._revision = self.catalog.revision()
        self._check_at = 0.
        self._process = self._requests = self._results = self._stopped = None
        self._transport = self._reader = None
        self._worker_future = None
        self._stop_future = None
        self._deferred_import = None
        self._epochs = None
        self._epoch_counts = dict(preview=0, content=0, import_=0)
        self._epoch_counts['import'] = self._epoch_counts.pop('import_')
        self._serial, self._work = 0, {}
        self._wanted = []
        self._selection_request = None
        self._import_request = self._write_future = None
        self._import_cancel = Event()
        self._import_game = game
        self._import_warnings = []
        self._closed = False
        self.search()

    @property
    def loading(self):
        return self._future is not None

    @property
    def importing(self):
        return self._import_request is not None or self._write_future is not None

    @property
    def writing(self):
        return self._write_future is not None and not self._write_future.done()

    def _bump(self, kind):
        self._epoch_counts[kind] += 1
        if self._epochs:
            self._epochs[kind].value = self._epoch_counts[kind]

    def search(self, filters: Filters | None = None, *, reset_selection=False):
        if self._closed:
            return
        self.filters = filters or self.filters
        self._cancel.set()
        if self._future:
            self._future[1].cancel()
        self._generation += 1
        self._cancel = Event()
        self.pages.clear()
        self._page_requests.clear()
        self.total = 0
        self.error = ''
        self.status = 'Loading library metadata…'
        self.pause()
        if reset_selection:
            self.select(None)
        self._query(0)

    def set_source(self, base: Path | None, game: str):
        self.catalog = Catalog(base, self.catalog.personal, self.catalog.cache_dir, game)
        self._revision = self.catalog.revision()
        self.categories, self.sources = [], []
        self.search(Filters(), reset_selection=True)

    def _query(self, offset):
        self._future = (self._generation, self._executor.submit(self.catalog.query, self.filters, offset, PAGE_SIZE, self._cancel))

    def asset_at(self, index):
        offset = index // PAGE_SIZE * PAGE_SIZE
        page = self.pages.get(offset)
        if page is not None:
            self.pages.move_to_end(offset)
            return page[index - offset] if index - offset < len(page) else None
        if offset not in self._page_requests and len(self._page_requests) < MAX_PAGES:
            self._page_requests.append(offset)
        return None

    def select(self, asset: Asset | None):
        if self.selection == asset:
            return
        self._bump('content')
        self.selection, self.geometry, self.shape, self.selection_error = asset, None, None, ''
        self._selection_request = None

    def want_previews(self, assets, size):
        wanted = [(a, size) for a in assets]
        if [(a.key, s) for a, s in wanted] != [(a.key, s) for a, s in self._wanted]:
            self._bump('preview')
            self._wanted = wanted

    def pause(self):
        self._wanted = []
        self._bump('preview')

    @staticmethod
    def _create_worker(counts):
        from ui.swf_editor.library.worker import run
        context = mp.get_context('spawn')
        requests, results, stopped = context.Queue(8), context.Queue(8), context.Event()
        epochs = {kind: context.Value('q', count) for kind, count in counts.items()}
        process = context.Process(target=run, args=(requests, results, stopped, epochs), name='swf-library-preview', daemon=True)
        try:
            process.start()
        except Exception:
            for channel in (requests, results):
                channel.cancel_join_thread()
                channel.close()
            raise
        delivered = queue.Queue(8)
        def receive():
            while not stopped.is_set():
                try:
                    result = results.get(timeout=.1)
                except queue.Empty:
                    continue
                except (EOFError, OSError, ValueError):
                    break
                while not stopped.is_set():
                    try:
                        delivered.put(result, timeout=.1)
                        break
                    except queue.Full:
                        continue
        reader = Thread(target=receive, name='swf-library-results', daemon=True)
        reader.start()
        return process, requests, delivered, stopped, epochs, results, reader

    def _ensure_worker(self):
        if self._stop_future:
            if not self._stop_future.done():
                return False
            self._stop_future.result()
            self._stop_future = None
        if self._process:
            return True
        if self._worker_future is None:
            self._worker_future = self._executor.submit(self._create_worker, dict(self._epoch_counts))
        if not self._worker_future.done():
            return False
        future, self._worker_future = self._worker_future, None
        self._process, self._requests, self._results, self._stopped, self._epochs, self._transport, self._reader = future.result()
        for kind, count in self._epoch_counts.items():
            self._epochs[kind].value = count
        return True

    def _submit(self, kind, **values):
        if len(self._work) >= 8:
            return None
        if not self._ensure_worker():
            return None
        self._serial += 1
        request = dict(id=self._serial, kind=kind, epoch=self._epoch_counts[kind], **values)
        try:
            self._requests.put_nowait(request)
        except queue.Full:
            return None
        self._work[self._serial] = request
        return self._serial

    def request_import(self, filename, threshold=128):
        if self.importing:
            return False
        self._import_cancel = Event()
        self._import_game = self.catalog.game
        self._import_warnings = []
        self._bump('import')
        self._deferred_import = dict(filename=str(filename), threshold=threshold)
        self._import_request = -1
        self.import_status = 'Reading and validating copied assets…'
        return True

    def save_assets(self, entries, *, game=None):
        if self.importing:
            return False
        self._import_cancel = Event()
        self._import_warnings = []
        self._write(entries, game or self.catalog.game)
        return True

    def _write(self, entries, game):
        self.import_status = 'Saving copied assets to your library…'
        self._write_future = self._executor.submit(add_personal, self.catalog.personal, game, entries, self._import_cancel)

    def cancel_import(self):
        active = self._import_request is not None and self._import_request > 0
        self._import_cancel.set()
        self._bump('import')
        self._import_request = None
        self._deferred_import = None
        if self._write_future:
            self._write_future.cancel()
        if active and not self._closed and self._process:
            bundle = self._detach_worker()
            future = self._stop_future = Future()
            def stop():
                try:
                    self._dispose_worker(bundle)
                    future.set_result(None)
                except Exception as exc:
                    future.set_exception(exc)
            Thread(target=stop, name='swf-library-stop', daemon=True).start()
        self.import_status = 'Import cancelled.'

    def poll(self):
        if self._closed:
            return
        if self._future and self._future[1].done():
            generation, future = self._future
            self._future = None
            try:
                page = future.result()
                if generation == self._generation:
                    self.pages[page.offset] = page.assets
                    while len(self.pages) > MAX_PAGES:
                        self.pages.popitem(last=False)
                    self.total, self._revision = page.total, page.revision
                    if page.offset == 0:
                        self.categories, self.sources = page.categories, page.sources
                    self.status = f'{self.total:,} assets'
            except (Cancelled, CancelledError):
                if generation == self._generation:
                    self.search(reset_selection=True)
            except Exception as exc:
                self.error, self.status = str(exc), 'Library unavailable'
        if self._future is None and self._page_requests and not self.error:
            offset = self._page_requests.popleft()
            if offset not in self.pages:
                self._query(offset)
        now = time.monotonic()
        if now >= self._check_at:
            self._check_at = now + 1
            if self.catalog.revision() != self._revision and not self.writing:
                self._revision = self.catalog.revision()
                self.search(reset_selection=True)
        if self._write_future and self._write_future.done():
            future, self._write_future = self._write_future, None
            try:
                count = future.result()
                self.import_status = f'Added {count:,} copied assets.'
                if self._import_warnings:
                    self.import_status += f' Skipped {len(self._import_warnings)} unsupported definitions: {self._import_warnings[0]}'
                self.search(reset_selection=True)
            except (Cancelled, CancelledError):
                self.import_status = 'Import cancelled; no assets added.'
            except Exception as exc:
                self.import_status = f'Import failed: {exc}'
        if self._process and not self._process.is_alive():
            self.selection_error = 'Preview helper exited. Select again or retry.'
            self.error = 'Library preview helper exited. Retry the library to restart it.'
            if self._import_request:
                self.import_status = 'Import failed: preview helper exited.'
            self._import_request = self._selection_request = None
            self._close_worker()
        for _ in range(8):
            if self._results is None:
                break
            try:
                result = self._results.get_nowait()
            except queue.Empty:
                break
            request = self._work.pop(result['id'], None)
            if request is None:
                continue
            stale = result.get('stale') or request['epoch'] != self._epoch_counts[request['kind']]
            if stale:
                self.metrics['stale_results'] += 1
                continue
            kind = result['kind']
            if kind == 'content' and result['id'] == self._selection_request:
                self.geometry, self.selection_error = result.get('geometry'), result.get('error', '')
                self.shape = result.get('shape')
                if result.get('warning'):
                    self.warnings[result['key']] = result['warning']
            elif kind == 'import' and result['id'] == self._import_request:
                self._import_request = None
                if result.get('error'):
                    self.import_status = 'Import failed: ' + result['error']
                else:
                    self._import_warnings = result['warnings']
                    self._write(result['entries'], self._import_game)
            elif kind == 'preview':
                key = (result['key'], result['size'])
                if result.get('error'):
                    self.errors[key] = result['error']
                else:
                    self.pixels[key] = result['pixels']
                    self.metrics['previews'] += 1
                    self.metrics['disk_hits'] += int(result['disk_hit'])
                    if result.get('warning'):
                        self.warnings[result['key']] = result['warning']
        while len(self.pixels) > 128 or sum(map(len, self.pixels.values())) > MAX_PIXELS_BYTES:
            self.pixels.popitem(last=False)
        for messages in (self.errors, self.warnings):
            while len(messages) > 512:
                messages.popitem(last=False)
        if self.error:
            return
        try:
            if self._deferred_import is not None:
                request = self._submit('import', **self._deferred_import)
                if request is not None:
                    self._import_request, self._deferred_import = request, None
            if self.selection and self.geometry is None and not self.selection_error and self._selection_request is None:
                self._selection_request = self._submit('content', asset=self.selection, key=self.selection.key)
            pending = {(r.get('key'), r.get('size')) for r in self._work.values() if r['kind'] == 'preview' and r['epoch'] == self._epoch_counts['preview']}
            for asset, size in self._wanted:
                key = (asset.key, size)
                if key in self.pixels:
                    self.pixels.move_to_end(key)
                if key not in self.pixels and key not in self.errors and key not in pending:
                    if sum(r['kind'] == 'preview' for r in self._work.values()) >= MAX_PREVIEWS:
                        break
                    if self._submit('preview', asset=asset, key=asset.key, size=size, cache=str(self.cache_dir / 'previews')) is None:
                        break
        except Exception as exc:
            self.error = f'Cannot start library preview: {exc}'

    def retry(self):
        self.errors.clear()
        self.selection_error, self._selection_request = '', None
        self.search()

    def _detach_worker(self):
        bundle = (self._process, self._requests, self._results, self._stopped, self._epochs, self._transport, self._reader)
        if self._stopped:
            self._stopped.set()
        self._process = self._requests = self._results = self._stopped = self._epochs = None
        self._transport = self._reader = None
        self._work.clear()
        self._selection_request = None
        return bundle

    @staticmethod
    def _dispose_worker(bundle):
        process, requests, _, stopped, _, transport, reader = bundle
        stopped.set()
        process.join(.05)
        if process.is_alive():
            process.terminate()
            process.join(.5)
        process.close()
        # Close the parent's unused writer to wake a receiver after interrupted
        # IPC; Queue.close alone does not close handles when it has no feeder.
        transport._writer.close()
        if reader:
            reader.join(.2)
        for channel in (requests, transport):
            channel.cancel_join_thread()
            channel.close()
            feeder = channel._thread
            if feeder:
                feeder.join(.2)
            # A producer's feeder owns its handles until it exits. Closing
            # them concurrently causes invalid-handle/semaphore errors.
            if feeder is None or not feeder.is_alive():
                channel._reader.close()
                channel._writer.close()
        if reader:
            reader.join(.2)

    def _close_worker(self):
        if self._process:
            self._dispose_worker(self._detach_worker())

    def close(self):
        if self._closed:
            return
        self._closed = True
        self._cancel.set()
        self.cancel_import()
        self._close_worker()
        if self._stop_future:
            self._stop_future.result(timeout=2)
            self._stop_future = None
        if self._worker_future:
            future, self._worker_future = self._worker_future, None
            if not future.cancel():
                def finish_start(started):
                    try:
                        self._dispose_worker(started.result())
                    except Exception:
                        pass
                future.add_done_callback(finish_start)
        self._executor.shutdown(wait=False, cancel_futures=True)
        self.pages.clear()
        self.pixels.clear()
