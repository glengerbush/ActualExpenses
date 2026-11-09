"""Synthetic saved-view input on the pinned 26.10 exported schema; no live database."""
import json,sqlite3,tempfile,zipfile,pathlib
root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
 with zipfile.ZipFile(root/'tests/fixtures/import-26.10.0.zip') as source:
  data={name:source.read(name) for name in source.namelist()}
 dbpath=pathlib.Path(tmp)/'db.sqlite';dbpath.write_bytes(data['db.sqlite']);db=sqlite3.connect(dbpath)
 account=db.execute('select id from accounts where name=?',('Spending',)).fetchone()[0]
 columns=[{'id':name,'hidden':hidden} for name,hidden in [('date',False),('payee',False),('notes',True),('payment',False),('deposit',False),('cleared',True),('balance',False)]]
 for key,value in [(f'hide-reconciled-{account}','true'),(f'transaction-table-columns-{account}',json.dumps(columns))]:db.execute('insert or replace into preferences(id,value) values(?,?)',(key,value))
 filters=[('12000000-0000-4000-8000-000000000010','Payments',[{'field':'amount','op':'isapprox','value':1250,'options':{'outflow':True}}]),('12000000-0000-4000-8000-000000000011','Monthly',[{'field':'date','op':'is','value':{'frequency':'monthly','start':'2026-01-02'}}]),('12000000-0000-4000-8000-000000000012','Notes',[{'field':'notes','op':'matches','value':'expense|refund'}])]
 filters.append(('12000000-0000-4000-8000-000000000013','Identity',[{'field':'account','op':'is','value':account}]))
 for id,name,conditions in filters:db.execute('insert into transaction_filters(id,name,conditions,conditions_op,tombstone) values(?,?,?,?,0)',(id,name,json.dumps(conditions),'and'))
 db.commit();db.close();data['db.sqlite']=dbpath.read_bytes()
 with zipfile.ZipFile(root/'tests/fixtures/register-settings-26.10.zip','w',compression=zipfile.ZIP_DEFLATED) as output:
  for name,content in data.items():info=zipfile.ZipInfo(name,(2026,1,1,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;output.writestr(info,content)
print('Generated register settings import fixture from pinned source archive')
