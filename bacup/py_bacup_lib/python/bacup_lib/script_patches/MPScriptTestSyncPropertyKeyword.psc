; Multiplayer replication test harness. OnSyncVariableNetworkChanged has no Fallout 4
; equivalent and nothing sets bShouldEnableRef locally, so the handler is dropped.
; ClientFunction() is left intact and callable.
; @drop-member OnSyncVariableNetworkChanged
