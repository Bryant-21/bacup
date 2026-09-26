Event OnTriggerEnter(ObjectReference akActionRef)
	If OnTriggerEnterLock || akActionRef == None
		Return
	EndIf
	OnTriggerEnterLock = True
	MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
	If MTNM04_RobotSentToWork_Keyword != None && akActionRef.HasKeyword(MTNM04_RobotSentToWork_Keyword)
		If questScript != None
			questScript.RobotArrivedAtWork(akActionRef as Actor)
		EndIf
	ElseIf akActionRef.HasKeyword(MTNM04_Undesirable_Keyword)
		Actor undesirable = akActionRef as Actor
		If UndesirablesInRoom != None && (undesirable == None || !undesirable.IsDead()) && UndesirablesInRoom.Find(akActionRef) < 0
			UndesirablesInRoom.AddRef(akActionRef)
		EndIf
		akActionRef.AddKeyword(MTNM04_UndesirableTriggered_Keyword)
		If questScript != None
			questScript.RefreshUndesirables()
		EndIf
	EndIf
	OnTriggerEnterLock = False
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If OnTriggerLeaveLock || akActionRef == None
		Return
	EndIf
	OnTriggerLeaveLock = True
	If UndesirablesInRoom != None && UndesirablesInRoom.Find(akActionRef) >= 0
		UndesirablesInRoom.RemoveRef(akActionRef)
		akActionRef.RemoveKeyword(MTNM04_UndesirableTriggered_Keyword)
		MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
		If questScript != None
			questScript.RefreshUndesirables()
		EndIf
	EndIf
	OnTriggerLeaveLock = False
EndEvent
