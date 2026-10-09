pub struct Unset;
pub struct Set<T>(T);

pub struct RequestBuilder<UrlState, MethodState> {
    url: UrlState,
    method: MethodState,
}

impl RequestBuilder<Unset, Unset> {
    pub fn new() -> Self {
        RequestBuilder { url: Unset, method: Unset }
    }
}

impl<M> RequestBuilder<Unset, M> {
    pub fn url(self, url: &str) -> RequestBuilder<Set<String>, M> {
        RequestBuilder { url: Set(url.to_string()), method: self.method }
    }
}

impl<U> RequestBuilder<U, Unset> {
    pub fn method(self, method: &str) -> RequestBuilder<U, Set<String>> {
        RequestBuilder { url: self.url, method: Set(method.to_string()) }
    }
}

impl RequestBuilder<Set<String>, Set<String>> {
    pub fn send(self) -> String {
        format!("Sending {} request to {}", self.method.0, self.url.0)
    }
}