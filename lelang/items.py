# Define here the models for your scraped items
#
# See documentation in:
# https://docs.scrapy.org/en/latest/topics/items.html

from itemloaders.processors import Compose, Join, MapCompose, TakeFirst
from w3lib.html import remove_tags


def get_id(url: str):
    import re

    return re.search(r"\/lot-lelang\/detail\/(\d+)\/", url).group(1)


def strip_currency(value: str):
    return value.replace("Rp", "").replace(".", "").strip()


def print_currency(value):
    return f"Rp {value:.}"


from datetime import datetime


MONTHS = {
    "Januari": "January",
    "Februari": "February",
    "Maret": "March",
    "April": "April",
    "Mei": "May",
    "Juni": "June",
    "Juli": "July",
    "Agustus": "August",
    "September": "September",
    "Oktober": "October",
    "November": "November",
    "Desember": "December",
}


def parse_datetime(input):
    # Replace Indonesian month names with English ones
    for id, en in MONTHS.items():
        input = input.replace(id, en)

    # Parse the datetime string into a datetime object
    format = "%d %B %Y jam %H:%M %Z"
    output = datetime.strptime(input, format)

    return output


def parse_date(input):
    # Replace Indonesian month names with English ones
    for id, en in MONTHS.items():
        input = input.replace(id, en)

    # Parse the date string into a date object
    format = "%d %B %Y"
    output = datetime.strptime(input, format).date()

    return output


from scrapy.item import Field, Item


class Property(Item):
    default_output_processor = TakeFirst()

    id = Field(
        input_processor=MapCompose(get_id),
    )
    url = Field()
    title = Field(
        input_processor=Compose(MapCompose(str.strip, remove_tags), Join(separator="")),
    )
    price = Field(
        input_processor=MapCompose(strip_currency),
    )
    collateral = Field(
        input_processor=MapCompose(strip_currency),
    )
    collateral_deadline = Field(
        input_processor=MapCompose(parse_date, lambda x: x.strftime("%Y-%m-%d")),
    )
    auction_type = Field()
    auction_deadline = Field(
        input_processor=MapCompose(parse_datetime, datetime.isoformat),
    )
    auction_code = Field()
    auction_by = Field()


from scrapy.loader import ItemLoader


class PropertyLoader(ItemLoader):
    default_item_class = Property
    default_output_processor = TakeFirst()
