Event OnTriggerEnter(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    ObjectReference playerRef = None
    If currentPlayer != None
        playerRef = currentPlayer.GetReference()
    EndIf
    If owningQuest == None || playerRef == None || akActionRef != playerRef
        Return
    EndIf
    If owningQuest.GetStage() < WeaselFollowingStage || owningQuest.GetStage() >= 1300
        Return
    EndIf
    StartTimer(1.0, SayTimerID)
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    ObjectReference playerRef = None
    If currentPlayer != None
        playerRef = currentPlayer.GetReference()
    EndIf
    If playerRef != None && akActionRef == playerRef
        CancelTimer(SayTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != SayTimerID
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.GetStage() < 1200 || owningQuest.GetStage() >= 1300
        Return
    EndIf
    If owningQuest.IsStageDone(PlayerTalkedToIntercomStage)
        Return
    EndIf
    ObjectReference playerRef = None
    ObjectReference intercomRef = None
    Actor louRef = None
    If currentPlayer != None
        playerRef = currentPlayer.GetReference()
    EndIf
    If LouIntercom != None
        intercomRef = LouIntercom.GetReference()
    EndIf
    If Lou != None
        louRef = Lou.GetActorReference()
    EndIf
    If playerRef == None || intercomRef == None || louRef == None || W05_MQR_201P_LouSaysTopic_IntercomGreeting == None
        Return
    EndIf
    intercomRef.Say(W05_MQR_201P_LouSaysTopic_IntercomGreeting, louRef, False, playerRef)
    owningQuest.SetStage(PlayerTalkedToIntercomStage)
EndEvent
