from urllib.parse import urlencode

import scrapy

from lelang.items import PropertyLoader

# https://lelang.go.id/lot-lelang?no-cache=O6mJaGoQr2gbB3ZNAkV0&order_by=terbaru&id_kategori%5B%5D=1&id_kategori%5B%5D=2&id_kategori%5B%5D=3&id_kategori%5B%5D=12&id_kategori%5B%5D=13&id_kategori%5B%5D=16&id_kategori%5B%5D=17&id_kategori%5B%5D=18&harga_minimum=&harga_maksimum=&id_kota%5B%5D=&id_provinsi%5B%5D=31&id_kota%5B%5D=3671&id_kota%5B%5D=3674&page=1

params = {
    "order_by": "terbaru",
    "page": 1,
    "id_kategori": [
        1,
        2,
        3,
        12,
        13,
        16,
        17,
        18,
    ],
    "harga_minimum": "",
    "harga_maksimum": "",
}

DB_FIELD_NAMES = {
    "Cara Penawaran": "auction_type",
    "Jaminan": "collateral",
    "Batas Akhir Jaminan": "collateral_deadline",
    "Batas Akhir Penawaran": "auction_deadline",
    "Penyelenggara": "auction_by",
    "Pelaksanaan Lelang": "auction_date",
    "Kode Lot Lelang": "auction_code",
}

DOMAIN = "lelang.go.id"


class PropertiesSpider(scrapy.Spider):
    name = "properties"
    allowed_domains = [DOMAIN]
    start_urls = [f"https://{DOMAIN}/lot-lelang?{urlencode(params)}"]

    def parse(self, response):
        links = response.css("#container-lot-lelang").css(".product .product-name a")
        yield from response.follow_all(links, self.parse_property)

        next_page = response.css(".pagination li.next a::attr(href)").get()
        if next_page is not None:
            yield response.follow(next_page, callback=self.parse)

    def parse_property(self, response):
        loader = PropertyLoader(selector=response.css(".product-essential"))
        loader.add_value("id", response.request.url)
        loader.add_value("url", response.request.url)
        loader.add_css("title", ".product-name *::text")
        loader.add_css("price", ".product-price::text")

        for row in response.css(".product-essential table.table tr"):
            from w3lib.html import remove_tags

            k, v = [remove_tags(td).strip() for td in row.css("td").getall()]
            loader.add_value(DB_FIELD_NAMES[k], v)

        yield loader.load_item()
