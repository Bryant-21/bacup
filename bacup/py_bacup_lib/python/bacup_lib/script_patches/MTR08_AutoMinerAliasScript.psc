Event OnActivate(ObjectReference akActionRef)
	Quest owner = GetOwningQuest()
	MTR08_MineScript mine = owner as MTR08_MineScript
	If mine != None
		mine.TryRepairAutoMiner(Self, akActionRef)
	EndIf
EndEvent

Event OnEnterBleedout()
	Parent.OnEnterBleedout()
	Quest owner = GetOwningQuest()
	MTR08_MineScript mine = owner as MTR08_MineScript
	If mine != None
		mine.HandleAutoMinerDown(Self)
	EndIf
EndEvent
