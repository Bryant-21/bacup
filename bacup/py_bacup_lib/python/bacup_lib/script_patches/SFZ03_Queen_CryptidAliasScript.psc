Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, Int aeCombatState)
	If aeCombatState > 0 && akSenderRef != None && akSenderRef.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
		SFZ03_Queen_QuestScript hunt = GetOwningQuest() as SFZ03_Queen_QuestScript
		If hunt != None
			hunt.HandleCryptidEncounter(akSenderRef, Self)
		EndIf
	EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
	If akTarget != None && akTarget.HasKeyword(SFZ03_Queen_CryptidBossKeyword)
		SFZ03_Queen_QuestScript hunt = GetOwningQuest() as SFZ03_Queen_QuestScript
		If hunt != None
			hunt.HandleCryptidEncounter(akTarget, Self)
		EndIf
	EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
	SFZ03_Queen_QuestScript hunt = GetOwningQuest() as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.HandleCryptidDeath(akSenderRef, Self)
	EndIf
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	SFZ03_Queen_QuestScript hunt = GetOwningQuest() as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.HarvestCryptidSample(akSenderRef, akActionRef)
	EndIf
EndEvent
