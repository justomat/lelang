# Define your item pipelines here
#
# Don't forget to add your pipeline to the ITEM_PIPELINES setting
# See: https://docs.scrapy.org/en/latest/topics/item-pipeline.html


# useful for handling different item types with a single interface
from itemadapter import ItemAdapter
from supabase import create_client, Client


class SupabasePipeline:
    table = "properties"

    def open_spider(self, spider):
        DB_URL = "https://vkckgovarpecuwkigiot.supabase.co"
        DB_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InZrY2tnb3ZhcnBlY3V3a2lnaW90Iiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImlhdCI6MTY5MDYyMjk3MiwiZXhwIjoyMDA2MTk4OTcyfQ.S5FSX0LK-6hjmP_9f_jBgIobI-EsTDHbVm2MX9R2w4I"
        self.db: Client = create_client(DB_URL, DB_KEY)

    def process_item(self, item, spider):
        data, count = (
            self.db.table(self.table).upsert(ItemAdapter(item).asdict()).execute()
        )
        return item
