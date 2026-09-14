Event OnQuestInit()
	ReadyStage = XPD_ObjectiveModule_ReadyStage_Global.GetValueInt()
	ActiveStage = XPD_ObjectiveModule_ActiveStage_Global.GetValueInt()
	Int index = 0
	While index < ProximityTriggers_RefCollAlias.GetCount()
		ObjectReference triggerRef = ProximityTriggers_RefCollAlias.GetAt(index)
		If triggerRef != None
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && GetStage() >= ReadyStage && GetStage() < ActiveStage
		SetStage(ActiveStage)
		If DisableAfterTriggering
			akSender.Disable()
		EndIf
	EndIf
EndEvent
