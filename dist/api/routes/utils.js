export function sendData(response, data, source) {
    const payload = {
        data,
        meta: {
            generatedAt: new Date().toISOString(),
            source,
        },
    };
    response.json(payload);
}
