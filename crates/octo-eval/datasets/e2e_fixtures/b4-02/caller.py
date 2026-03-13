from src import fetch_data


def process_url(url):
    result = fetch_data(url)
    return result[0]  # BUG: expects list but fetch_data returns dict; should use result["data"]
