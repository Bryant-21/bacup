Event OnKill(Actor akVictim)
	If akVictim != None && akVictim.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
		SFZ03_Queen_QuestScript hunt = GetOwningQuest() as SFZ03_Queen_QuestScript
		If hunt != None
			hunt.HandleCryptidDeath(akVictim, None)
		EndIf
	EndIf
EndEvent
