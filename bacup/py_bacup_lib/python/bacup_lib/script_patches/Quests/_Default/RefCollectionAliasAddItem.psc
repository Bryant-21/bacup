Event OnAliasInit()
	OwningQuest = Self.GetOwningQuest()
	If AddOnAliasInit
		AddToAll()
	EndIf
	If StageToAdd >= 0 && OwningQuest
		If OwningQuest.IsStageDone(StageToAdd)
			AddToAll()
		Else
			Self.RegisterForRemoteEvent(OwningQuest, "OnStageSet")
		EndIf
	EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
	If auiStageID == StageToAdd
		Self.UnregisterForRemoteEvent(akSender, "OnStageSet")
		AddToAll()
	EndIf
EndEvent

Event OnDying(ObjectReference akSenderRef, Actor akKiller)
	If AddOnDying
		AddTo(akSenderRef)
	EndIf
EndEvent

Function AddToAll()
	Int i = 0
	Int count = Self.GetCount()
	While i < count
		AddTo(Self.GetAt(i))
		i += 1
	EndWhile
EndFunction

Function AddTo(ObjectReference akRef)
	If !akRef || !ItemToAdd
		Return
	EndIf
	If ChanceToAdd < 1.0 && Utility.RandomFloat(0.0, 1.0) >= ChanceToAdd
		Return
	EndIf
	akRef.AddItem(ItemToAdd, NumToAdd, AddSilently)
EndFunction
