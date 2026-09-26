; One power plant subsystem (reactor 0, generator 1, cooling 2) for Powering Up.
;
; IntactCollection holds every *Breakable_PowerPlant* ACTI of the subsystem, filled from the
; plant location's LCSR entries (PowerPlantReactorDestructibleObject 01849C and its generator /
; cooling siblings). Each of those activators runs DefaultFixable2StateActivator, so the world is
; the authority for what is broken: the intact percentage is measured with IsActivatorBroken()
; rather than from a counter, which keeps it correct across save/load and across an event that
; failed and left the plant damaged. DestroyedCollection mirrors the broken ones because the
; quest's objective targets point at it, so the Pip-Boy marks exactly the leftovers.
;
; The script is also bound to the CoolingQTs alias (27) in the source data with the same
; collection properties, which is why everything works through IntactCollection instead of Self.

RefCollectionAlias Function GetSubsystemCollection()
	If IntactCollection != None
		Return IntactCollection
	EndIf
	Return Self as RefCollectionAlias
EndFunction

DefaultFixable2StateActivator Function GetBreakableAt(Int aiIndex)
	RefCollectionAlias collection = GetSubsystemCollection()
	If collection == None
		Return None
	EndIf
	Return collection.GetAt(aiIndex) as DefaultFixable2StateActivator
EndFunction

Int Function CountBrokenObjects()
	RefCollectionAlias collection = GetSubsystemCollection()
	If collection == None
		Return 0
	EndIf
	Int brokenCount = 0
	Int index = 0
	Int total = collection.GetCount()
	While index < total
		DefaultFixable2StateActivator breakable = GetBreakableAt(index)
		If breakable != None && breakable.IsActivatorBroken()
			brokenCount += 1
		EndIf
		index += 1
	EndWhile
	Return brokenCount
EndFunction

Int Function GetIntactPercent()
	RefCollectionAlias collection = GetSubsystemCollection()
	If collection == None
		Return 100
	EndIf
	Int total = collection.GetCount()
	If total <= 0
		Return 100
	EndIf
	Return ((total - CountBrokenObjects()) * 100) / total
EndFunction

Function SyncDestroyedCollection()
	If DestroyedCollection == None
		Return
	EndIf
	RefCollectionAlias collection = GetSubsystemCollection()
	If collection == None
		Return
	EndIf
	Int index = 0
	Int total = collection.GetCount()
	While index < total
		ObjectReference member = collection.GetAt(index)
		DefaultFixable2StateActivator breakable = member as DefaultFixable2StateActivator
		If member != None && breakable != None
			Bool isBroken = breakable.IsActivatorBroken()
			Bool isTracked = DestroyedCollection.Find(member) >= 0
			If isBroken && !isTracked
				DestroyedCollection.AddRef(member)
			ElseIf !isBroken && isTracked
				DestroyedCollection.RemoveRef(member)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function NotifyEventQuest()
	If eventQuest == None
		eventQuest = GetOwningQuest() as powerplanteventquestscript
	EndIf
	If eventQuest != None
		eventQuest.NotifySubsystemStateChanged(SubsystemID)
	EndIf
EndFunction

; Break objects until at most afThresholdPercent of the subsystem is left intact. Never repairs,
; so a plant that was left damaged by a failed event keeps the damage it had.
Function DestroySubsystemToThreshold(Float afThresholdPercent, Bool abDestroySilently)
	If isDestroyFunctionActive
		Return
	EndIf
	isDestroyFunctionActive = True
	RefCollectionAlias collection = GetSubsystemCollection()
	Int total = 0
	If collection != None
		total = collection.GetCount()
	EndIf
	If total > 0
		Int thresholdPercent = afThresholdPercent as Int
		If thresholdPercent < 0
			thresholdPercent = 0
		ElseIf thresholdPercent > 100
			thresholdPercent = 100
		EndIf
		Int targetBrokenCount = total - ((total * thresholdPercent) / 100)
		Int brokenCount = CountBrokenObjects()
		Int startOffset = Utility.RandomInt(0, total - 1)
		Int step = 0
		While step < total && brokenCount < targetBrokenCount
			DefaultFixable2StateActivator breakable = GetBreakableAt((startOffset + step) % total)
			If breakable != None && !breakable.IsActivatorBroken()
				breakable.SetActivatorBroken(True, abDestroySilently)
				brokenCount += 1
			EndIf
			step += 1
		EndWhile
	EndIf
	SyncDestroyedCollection()
	isDestroyFunctionActive = False
	NotifyEventQuest()
EndFunction

Function DestroySubsystemToThresholdNoWait(Float afThresholdPercent, Bool abDestroySilently)
	DestroySubsystemToThresholdNoWait_damageThresholdPercent = afThresholdPercent
	DestroySubsystemToThresholdNoWait_destroySilently = abDestroySilently
	CancelTimer(CONST_DestroySubsystemToThresholdNoWaitTimerID)
	StartTimer(0.1, CONST_DestroySubsystemToThresholdNoWaitTimerID)
EndFunction

Function RepairSubsystem()
	RefCollectionAlias collection = GetSubsystemCollection()
	Int total = 0
	If collection != None
		total = collection.GetCount()
	EndIf
	Int index = 0
	While index < total
		DefaultFixable2StateActivator breakable = GetBreakableAt(index)
		If breakable != None && breakable.IsActivatorBroken()
			breakable.SetActivatorBroken(False, True)
		EndIf
		index += 1
	EndWhile
	If DestroyedCollection != None
		DestroyedCollection.RemoveAll()
	EndIf
	NotifyEventQuest()
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == CONST_DestroySubsystemToThresholdNoWaitTimerID
		DestroySubsystemToThreshold(DestroySubsystemToThresholdNoWait_damageThresholdPercent, DestroySubsystemToThresholdNoWait_destroySilently)
	EndIf
EndEvent

Event OnAliasInit()
	eventQuest = GetOwningQuest() as powerplanteventquestscript
	RefCollectionAlias collection = GetSubsystemCollection()
	If collection != None
		totalObjectCount = collection.GetCount()
	EndIf
	isDestroyFunctionActive = False
	hasInitialized = True
EndEvent

Event OnAliasShutdown()
	CancelTimer(CONST_DestroySubsystemToThresholdNoWaitTimerID)
	isDestroyFunctionActive = False
	hasInitialized = False
EndEvent

; The activators repair themselves when the player activates them; the alias repairs any straggler
; so the count cannot depend on which script instance handled the activation first.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	If akSenderRef == None || akActionRef != Game.GetPlayer()
		Return
	EndIf
	DefaultFixable2StateActivator breakable = akSenderRef as DefaultFixable2StateActivator
	If breakable == None
		Return
	EndIf
	If breakable.IsActivatorBroken()
		breakable.SetActivatorBroken(False, False)
	EndIf
	If DestroyedCollection != None && DestroyedCollection.Find(akSenderRef) >= 0
		DestroyedCollection.RemoveRef(akSenderRef)
	EndIf
	NotifyEventQuest()
EndEvent
