Event OnInit()
	ValveRef = GetLinkedRef(LinkCustom01)
	ValveSteamRef = GetLinkedRef(LinkCustom02)
	DoorRef = GetLinkedRef(LinkCustom03)
	myActivator = ValveRef as default2stateactivator

	if ValveRef != None
		RegisterForRemoteEvent(ValveRef, "OnActivate")
	endif
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	if akSender != ValveRef || akActivator != Game.GetPlayer()
		return
	endif

	bDoorToggle = True
	if ValveSteamRef != None
		ValveSteamRef.EnableNoWait()
		if SteamSound != None
			SteamSound.Play(ValveSteamRef)
		endif
	endif
	StartTimer(SteamTimerLength, SteamTimerID)
	StartTimer(DoorTimerLength, DoorTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
	if aiTimerID == SteamTimerID
		if ValveSteamRef != None
			ValveSteamRef.DisableNoWait()
		endif
	elseif aiTimerID == DoorTimerID
		bDoorToggle = False
	endif
EndEvent

Event OnReset()
	bDoorToggle = False
	if ValveSteamRef != None
		ValveSteamRef.DisableNoWait()
	endif
EndEvent
