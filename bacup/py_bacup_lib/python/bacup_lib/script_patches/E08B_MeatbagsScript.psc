Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
	If akSenderRef == None || !akSenderRef.IsDestroyed()
		Return
	EndIf
	Quest owner = GetOwningQuest()
	E08B_EvictionNoticeScript eventScript = owner as E08B_EvictionNoticeScript
	If eventScript != None
		eventScript.MeatbagDestroyed(akSenderRef)
	EndIf
EndEvent
