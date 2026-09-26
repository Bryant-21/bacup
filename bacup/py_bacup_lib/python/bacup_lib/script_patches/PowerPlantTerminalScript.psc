; Master control terminal for Powering Up (PowerPlant_TerminalMonongah 3EB68F,
; PowerPlant_TerminalPoseidon 3EB685, PowerPlant_TerminalThunderMt 3EB68E).
;
; The record drives itself off two actor values on the terminal reference, which is why the event
; quest pushes state in here instead of the terminal polling anything:
;   PowerPlant_TerminalStatusActorValue 3EB68C - 0 body text "STATUS: ACTIVE", 1 "SYSTEM FAILURE",
;     2 "OFFLINE" (ready for restart);
;   PowerPlant_RestartAvailableActorValue 4374FB - gates both "Restart Power Plant" items.
; Menu item 7 (status 1) prints ERROR 144, menu item 1 (status 2) prints the successful restart
; sequence and is the item FO76 bound Fragment_Terminal_01 to, so the restart runs from item 1.
;
; PowerPlantMsg_StatusFailure / StatusDash / StatusBlank feed FO76's "<Token=...>" readout tokens,
; which FO4 text replacement does not implement; those three properties stay unused.

Int Function GetPowerPlantTerminalStatus(ObjectReference akTerminalRef)
	If akTerminalRef == None || PowerPlant_TerminalStatusActorValue == None
		Return CONST_TerminalStatus_Normal
	EndIf
	Return akTerminalRef.GetValue(PowerPlant_TerminalStatusActorValue) as Int
EndFunction

Function ApplyPowerPlantTerminalState(ObjectReference akTerminalRef, Int aiStatus, Bool abRestartAvailable)
	If akTerminalRef == None
		Return
	EndIf
	If PowerPlant_TerminalStatusActorValue != None
		akTerminalRef.SetValue(PowerPlant_TerminalStatusActorValue, aiStatus as Float)
	EndIf
	If PowerPlant_RestartAvailableActorValue != None
		Int availability = CONST_RestartAvailableStatus_NotAvailable
		If abRestartAvailable
			availability = CONST_RestartAvailableStatus_Available
		EndIf
		akTerminalRef.SetValue(PowerPlant_RestartAvailableActorValue, availability as Float)
	EndIf
EndFunction

Function SetPowerPlantTerminalOnline(ObjectReference akTerminalRef)
	ApplyPowerPlantTerminalState(akTerminalRef, CONST_TerminalStatus_Normal, False)
EndFunction

Function SetPowerPlantTerminalNeedsRepair(ObjectReference akTerminalRef, Bool abRestartOffered)
	ApplyPowerPlantTerminalState(akTerminalRef, CONST_TerminalStatus_SubsystemFailure, abRestartOffered)
EndFunction

Function SetPowerPlantTerminalReadyForRestart(ObjectReference akTerminalRef)
	ApplyPowerPlantTerminalState(akTerminalRef, CONST_TerminalStatus_ReadyForRestart, True)
EndFunction

Function RequestPowerPlantRestart(ObjectReference akTerminalRef)
	If GetPowerPlantTerminalStatus(akTerminalRef) != CONST_TerminalStatus_ReadyForRestart
		Return
	EndIf
	powerplantmasterquestscript master = PowerPlantMaster as powerplantmasterquestscript
	If master == None
		Return
	EndIf
	powerplanteventquestscript activeEvent = master.GetActivePowerPlantEvent(PowerPlantIndex)
	If activeEvent == None
		Return
	EndIf
	activeEvent.RestartPowerPlant()
EndFunction

Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID == 1
		RequestPowerPlantRestart(akTerminalRef)
	EndIf
EndEvent
