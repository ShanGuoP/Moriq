// Local ONNX inference worker. Model acquisition is owned by the Rust application.
import fs from 'node:fs/promises';
import readline from 'node:readline';
import path from 'node:path';
import { env, AutoModel, AutoProcessor, AutoTokenizer, Florence2ForConditionalGeneration, RawImage } from '@huggingface/transformers';
import * as ort from 'onnxruntime-node';
import { recognizeFaces } from './faces.mjs';
env.allowRemoteModels = false;
env.allowLocalModels = true;
env.useBrowserCache = false;
const loaded = new Map();
let busy = false;
let keepLoaded = false;
let lastUsed = Date.now();
setInterval(() => { if (!keepLoaded && !busy && Date.now() - lastUsed > 30000) process.exit(0); }, 5000).unref();
const normalized = (data) => { const values = Array.from(data, Number); const n = Math.hypot(...values); if (!n || !Number.isFinite(n)) throw new Error('模型返回无效向量'); return values.map(v => v / n); };
async function load(kind, root) {
    const key = `${kind}:${root}`;
    if (loaded.has(key)) return loaded.get(key);
    for (const old of loaded.values()) { await old.model?.dispose?.(); await old.detector?.release?.(); await old.recognizer?.release?.(); }
    loaded.clear();
    const supported = ort.listSupportedBackends().filter(b => b.bundled).map(b => b.name);
    // ORT 1.21 DirectML produces invalid tokens with the pinned quantized Florence
    // decoder. Keep its CPU path; CLIP/faces and other backends remain eligible.
    const preferred = ['dml', 'cuda', 'coreml'].find(name => supported.includes(name) && !(kind === 'florence' && name === 'dml'));
    let value;
    let lastError;
    for (const device of [...(preferred ? [preferred] : []), 'cpu']) {
        const candidate = { provider: device };
        try {
            if (kind === 'clip') {
                candidate.model = await AutoModel.from_pretrained(root, { dtype: 'q8', device, local_files_only: true });
                candidate.processor = await AutoProcessor.from_pretrained(root, { local_files_only: true });
                candidate.tokenizer = await AutoTokenizer.from_pretrained(root, { local_files_only: true });
            } else if (kind === 'florence') {
                candidate.model = await Florence2ForConditionalGeneration.from_pretrained(root, { dtype: 'q8', device, local_files_only: true });
                candidate.processor = await AutoProcessor.from_pretrained(root, { local_files_only: true });
            } else if (kind === 'faces') {
                candidate.detector = await ort.InferenceSession.create(path.join(root, 'yunet/face_detection_yunet_2023mar.onnx'), { executionProviders: [device], intraOpNumThreads: 2 });
                candidate.recognizer = await ort.InferenceSession.create(path.join(root, 'sface/face_recognition_sface_2021dec.onnx'), { executionProviders: [device], intraOpNumThreads: 2 });
            } else throw new Error('不支持的模型任务');
            value = candidate;
            break;
        } catch (error) {
            lastError = error;
            await candidate.model?.dispose?.();
            await candidate.detector?.release?.();
            await candidate.recognizer?.release?.();
        }
    }
    if (!value) throw lastError ?? new Error('没有可用的本地推理设备');
    loaded.set(key, value);
    return value;
}
async function run(request) {
    if (!['clip', 'florence', 'faces'].includes(request.operation)) throw new Error('无效的分析任务');
    if (typeof request.model_root !== 'string' || !path.isAbsolute(request.model_root)) throw new Error('模型目录必须是本地绝对路径');
    const session = await load(request.operation, request.model_root);
    const image = request.image ? await RawImage.read(request.image) : new RawImage(new Uint8ClampedArray(224 * 224 * 3).fill(255), 224, 224, 3);
    if (request.operation === 'clip') {
        const labels = request.text ? [request.text] : (request.labels?.length ? request.labels : ['人像', '风景', '建筑', '插画', '截图', '动物', '植物', '美食', '夜景']);
        if (labels.length > 128 || labels.some(s => typeof s !== 'string' || s.length > 4096)) throw new Error('分类词数量或长度超出范围');
        const inputs = { ...await session.processor(image), ...await session.tokenizer(labels, { padding: true, truncation: true }) };
        const output = await session.model(inputs);
        const logits = Array.from(output.logits_per_image.data, Number);
        const max = Math.max(...logits); const weights = logits.map(v => Math.exp(v - max)); const sum = weights.reduce((a,b) => a+b, 0);
        return { provider: session.provider, image_vector: normalized(output.image_embeds.data), text_vector: request.text ? normalized(output.text_embeds.data) : [], labels: labels.map((label,i) => ({ label, score: weights[i] / sum })).sort((a,b) => b.score-a.score) };
    }
    if (request.operation === 'faces') return { provider: session.provider, faces: await recognizeFaces(image, session.detector, session.recognizer) };
    const result = { provider: session.provider };
    const tasks = request.tasks ?? ['caption', 'ocr', 'objects'];
    for (const task of tasks) {
        const token = { caption: '<MORE_DETAILED_CAPTION>', ocr: '<OCR_WITH_REGION>', objects: '<OD>' }[task];
        if (!token) throw new Error('不支持的视觉分析任务');
        const prompts = session.processor.construct_prompts(token);
        const inputs = await session.processor(image, prompts);
        const ids = await session.model.generate({ ...inputs, max_new_tokens: task === 'ocr' ? 512 : 256, do_sample: false, num_beams: 1 });
        const text = session.processor.batch_decode(ids, { skip_special_tokens: false })[0];
        const structured = session.processor.post_process_generation(text, token, image.size)[token];
        if (task === 'caption') result.caption = typeof structured === 'string' ? structured : JSON.stringify(structured);
        if (task === 'ocr') { result.ocr = structured?.labels?.join('\n') ?? ''; result.ocr_regions = structured; }
        if (task === 'objects') result.objects = structured;
    }
    return result;
}
// stdout is exclusively a bounded request/response protocol; library diagnostics use stderr.
for await (const line of readline.createInterface({ input: process.stdin, crlfDelay: Infinity })) {
    let id = null;
    try {
        if (line.length > 2 * 1024 * 1024) throw new Error('任务请求超出预算');
        const request = JSON.parse(line); id = request.id; busy = true; keepLoaded = request.keep_loaded === true;
        const result = await run(request);
        process.stdout.write(JSON.stringify({ id, result }) + '\n');
    } catch (error) {
        process.stdout.write(JSON.stringify({ id, error: String(error?.message ?? error).slice(0, 4096) }) + '\n');
    } finally { busy = false; lastUsed = Date.now(); }
}
