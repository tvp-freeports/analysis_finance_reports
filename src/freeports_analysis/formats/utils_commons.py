def normalize_string(string: str):
    string = string.strip()
    string = string.lower()
    string = " ".join(string.split())
    return string


def normalize_word(word: str):
    word = word.strip()
    word = "".join(word.split())
    return word
