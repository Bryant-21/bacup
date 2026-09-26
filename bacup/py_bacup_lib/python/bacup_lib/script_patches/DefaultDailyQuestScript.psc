; DailyQuestDoneAV is declared mandatory and bound on every daily, and the FO76 player
; alias fills on "GetActorValue(DailyQuestDoneAV) < GLOB 117B42 SQ_TimestampToday", so
; the AV is a day stamp, not a 0/1 flag. SQ_MasterScript keeps SQ_TimestampToday at
; (GetCurrentGameTime() + 1) and advances it every game day, so stamping today's value
; on completion reads "already done today" and clears itself tomorrow.
;
; Guarded on aiToday > 0 so a scheduler that never ran cannot stamp a value that no
; later comparison can beat -- an unstamped AV sits at 0 and every daily stays startable,
; which is exactly today's behaviour.
;
; NOTE for consumers: on every daily I measured, the converter replaced that alias's
; FO76 event fill with a forced reference to Fallout4.esm:000014, and FO4 does not
; evaluate conditions on a forced-reference fill. So this stamp is a correct shared
; source of truth to condition on, but it does NOT by itself gate a second run today.
Function StampDailyQuestDone()
    If DailyQuestDoneAV == None
        Return
    EndIf
    GlobalVariable todayGlobal = Game.GetFormFromFile(0x00117B42, "SeventySix.esm") as GlobalVariable
    If todayGlobal == None
        Return
    EndIf
    Int aiToday = todayGlobal.GetValueInt()
    If aiToday <= 0
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(DailyQuestDoneAV, aiToday as Float)
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If IsCompleted()
        StampDailyQuestDone()
    EndIf
EndEvent

Event OnStoryScript(Keyword akKeyword, Location akLocation, ObjectReference akRef1, ObjectReference akRef2, Int aiValue1, Int aiValue2)
	Debug.Trace("[B21 Daily] DefaultDailyQuest OnStoryScript quest=" + Self as String + " keyword=" + akKeyword as String + " location=" + akLocation as String, 0)
	If akKeyword == SQ_RegionDailyQuestKeyword
		If StartingStage_DailyQuestKeyword >= 0
			Debug.Trace("[B21 Daily] DefaultDailyQuest setting daily stage=" + StartingStage_DailyQuestKeyword as String + " quest=" + Self as String, 0)
			SetStage(StartingStage_DailyQuestKeyword)
			Debug.Trace("[B21 Daily] DefaultDailyQuest daily stage result currentStage=" + GetStage() as String + " running=" + IsRunning() as String, 0)
		Else
			Debug.Trace("[B21 Daily] DefaultDailyQuest ignored daily event: starting stage is negative", 0)
		EndIf
	ElseIf StartingStage_OtherKeyword >= 0
		Debug.Trace("[B21 Daily] DefaultDailyQuest setting other stage=" + StartingStage_OtherKeyword as String + " quest=" + Self as String, 0)
		SetStage(StartingStage_OtherKeyword)
	Else
		Debug.Trace("[B21 Daily] DefaultDailyQuest ignored unmatched story event keyword=" + akKeyword as String, 0)
	EndIf
EndEvent
