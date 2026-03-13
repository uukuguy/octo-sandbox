from src import fetch_data


def process_url(url):
    result = fetch_data(url)
    return result["data"]
