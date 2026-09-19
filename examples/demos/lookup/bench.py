import json
from workload import workload

print("METRIC " + json.dumps({"name": "operations", "value": workload()[1], "unit": "ops"}))
