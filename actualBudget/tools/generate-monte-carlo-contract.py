"""Generate the Monte Carlo contract from the native saved-field validation table.

The field table is declarative; fail if its structure changes. Financial results
are described independently from the pinned engine's exported result types.
"""
import copy
import json
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
text = (root / 'server/src/monte_carlo/input.rs').read_text()
pairs = lambda value: re.findall(r'\(\s*"([^"]+)",\s*"([^"]+)"\s*,?\s*\)', value)
groups = {name: pairs(body) for name, body in re.findall(r'"(\w+)" => &\[(.*?)\],', text, re.S)}
plan = re.search(r'object\(\s*config,\s*&\[(.*?)\],\s*\)', text, re.S)
assert plan and set(groups) == {'pots', 'income', 'contributions', 'phases', 'bands', 'rule'}
groups['plan'] = pairs(plan[1])
assert len(groups['plan']) == 16

def obj(properties, required=None):
    return dict(type='object', additionalProperties=False, properties=properties,
                required=list(properties) if required is None else required)

def array(items, maximum):
    return dict(type='array', items=items, maxItems=maximum)

def ref(name):
    return {'$ref': '#/components/schemas/MonteCarlo' + name}

names = dict(pots='Pot', income='Income', contributions='Contribution', phases='Phase', bands='TaxBand', rule='WithdrawalRule', plan='Plan')
schemas = {}
for kind, fields in groups.items():
    properties = {}
    for field, field_type in fields:
        if field_type == 'number':
            schema = dict(type=['number', 'null'], minimum=-9007199254740991, maximum=9007199254740991)
        elif field_type == 'text':
            schema = dict(type=['string', 'null'], maxLength=4096)
        elif field_type == 'bool':
            schema = dict(type=['boolean', 'null'])
        elif field_type in groups:
            item = ref(names[field_type])
            schema = {'anyOf': [item if field_type == 'rule' else array(item, 256), {'type': 'null'}]}
        else:
            schema = {'enum': field_type.split('|') + [None]}
        properties[field] = schema
    schemas['MonteCarlo' + names[kind]] = obj(properties, [])

number, integer = {'type': 'number'}, {'type': 'integer'}
nullable_number = {'type': ['number', 'null']}
uuid = {'type': 'string', 'format': 'uuid', 'pattern': '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'}
detail = {key: number for key in 'year startBalance withdrawal plannedSpending spent growth endBalance income incomeTax unspentIncome surplusSaved contributions taxPaid feesPaid inaccessibleBalance'.split()}
for key in 'potBalances potStartBalances incomeAmounts potContributions contributionAmounts potWithdrawals potTaxes potTaxables potFees'.split():
    detail[key] = array(number, 256)
detail['potReturns'] = array(nullable_number, 256)
detail['inflation'] = nullable_number
detail['minimumApplied'] = detail['afterDepletion'] = {'type': 'boolean'}
detail['ruleExplanation'] = {'oneOf': [
    obj({'kind': {'const': 'anchor'}, 'rate': number}),
    obj({'kind': {'const': 'floor-ceiling'}, **{key: number for key in ['rate', 'unclamped', 'floor', 'ceiling']}, 'applied': {'enum': ['floor', 'ceiling', 'rate']}}),
    obj({'kind': {'const': 'factor'}, 'rule': {'enum': ['guardrails', 'ratcheting', 'boundaries']}, **{key: number for key in ['factor', 'planned', 'adjusted', 'currentRate', 'referenceRate', 'ratchetStreak']}, 'action': {'enum': ['cut', 'raise', 'none']}}, ['kind', 'rule', 'factor', 'planned', 'adjusted', 'action']),
]}
optional_detail = {'minimumApplied', 'afterDepletion', 'ruleExplanation', 'inaccessibleBalance'}
schemas['MonteCarloRunDetail'] = obj(detail, [key for key in detail if key not in optional_detail])
result = {
    'successRate': dict(type='number', minimum=0, maximum=1),
    'percentileBands': array(obj({key: integer for key in 'year p5 p10 p25 p30 p50 p70 p75 p90'.split()}), 101),
    'depletionHistogram': array(obj({'year': integer, 'count': integer}), 100),
    'depletionProbabilityByYear': array(number, 101),
    'medianEndingBalance': integer, 'medianTotalWithdrawn': integer,
    **{key: {'type': ['integer', 'null']} for key in ['medianDepletionYear', 'earliestDepletionYear', 'latestDepletionYear']},
    'worstRunPath': array(number, 101), 'endingBalances': array(number, 10000),
    'depletionYearBySimulation': array(integer, 10000), 'totalWithdrawnBySimulation': array(number, 10000),
    'runDetail': array(ref('RunDetail'), 100),
    'simulationCount': dict(type='integer', minimum=1, maximum=10000),
    'horizonYears': dict(type='integer', minimum=1, maximum=100),
}
schemas['MonteCarloResult'] = obj(result, [key for key in result if key != 'runDetail'])
schemas['MonteCarloRequest'] = obj({
    'version': {'const': 1}, 'request_id': uuid,
    'config': {'anyOf': [ref('Plan'), {'type': 'null'}]},
    'seed': dict(type='integer', minimum=0, maximum=4294967295, default=1234),
    'captureRunDetail': dict(type=['integer', 'null'], minimum=0, maximum=9999),
    'deflateToTodaysMoney': dict(type='boolean', default=False),
}, ['version', 'request_id', 'config'])
schemas['MonteCarloResponse'] = obj({'data': obj({
    'config': ref('Plan'), 'result': ref('Result'), 'snapshot': {'type': 'string'}, 'epoch': {'type': 'string'},
    'recorded': {'const': False}, 'cache': obj({'hit': {'type': 'boolean'}, 'input_revision': {'type': 'string'}}),
    'warnings': array(obj({'code': {'const': 'account_unavailable'}, 'pot_id': {'type': ['string', 'null']}, 'message': {'type': 'string'}}), 256),
    'assumptions': array({'type': 'string'}, 20),
})})
response = lambda schema, description: {'description': description, 'content': {'application/json': {'schema': schema}}}
errors = {str(code): response({'$ref': '#/components/schemas/Error'}, reason) for code, reason in [(400, 'Invalid plan or resource bound'), (403, 'Browser origin rejected'), (404, 'Budget or active request missing'), (409, 'Feature disabled, duplicate active request or simulation cancelled'), (410, 'Budget deleted'), (429, 'Simulation worker busy'), (500, 'Calculation unavailable')]}
budget_parameter = {'name': 'budget', 'in': 'path', 'required': True, 'schema': uuid}
path = '/api/v1/budgets/{budget}/reports/monte-carlo'
paths = {path: {'post': {
    'description': 'Experimental opt-in; read-only seeded planning. Input64KiB,256 entries per collection; source1000–10000 random runs and1–100 years (historical replay uses98 starts). One global worker,64MiB runtime heap,1MiB stack,20s execution interruption,8MiB output. Captures linked balances/epoch/snapshot before computing outside SQLite. Cache8 entries/16MiB keyed by resolved inputs and epoch; no financial writes. Cancel using the client-generated request_id; requests and statuses are ephemeral. Financial summaries are estimates, not guaranteed outcomes.',
    'parameters': [budget_parameter],
    'requestBody': {'required': True, 'content': {'application/json': {'schema': ref('Request')}}},
    'responses': {**errors, '200': response(ref('Response'), 'Captured simulation and assumptions')},
}}}
parameters = [budget_parameter, {'name': 'request_id', 'in': 'path', 'required': True, 'schema': uuid}]
paths[path + '/{request_id}'] = {
    'get': {'parameters': parameters, 'description': 'Read a currently active request;404 after completion or cancellation finishes.', 'responses': {**errors, '200': response(obj({'data': obj({'request_id': uuid, 'status': {'enum': ['running', 'cancelling']}})}), 'Active request')}},
    'delete': {'parameters': parameters, 'description': 'Interrupt active calculation. The original POST returns409 simulation_cancelled. No financial mutation or durable cancellation record is created.', 'responses': {**errors, '200': response(obj({'data': obj({'request_id': uuid, 'cancellation_requested': {'const': True}})}), 'Cancellation requested')}},
}
fragment = {'paths': paths, 'components': {'schemas': schemas}}
output = root / 'server/src/monte_carlo/openapi.json'
combined_path = root / 'server/src/openapi.json'
combined = json.loads(combined_path.read_text())
if '--check' in sys.argv:
    assert json.loads(output.read_text()) == fragment
    for path, value in paths.items(): assert combined['paths'][path] == value
    for name, value in schemas.items(): assert combined['components']['schemas'][name] == value
else:
    output.write_text(json.dumps(fragment, indent=2) + '\n')
    combined['paths'].update(copy.deepcopy(paths))
    combined['components']['schemas'].update(copy.deepcopy(schemas))
    combined_path.write_text(json.dumps(combined, indent=2) + '\n')
print('PASS Monte Carlo request/result/cancellation contracts match native input declarations')
