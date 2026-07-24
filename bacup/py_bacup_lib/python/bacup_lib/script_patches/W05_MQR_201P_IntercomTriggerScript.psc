; TODO

Event OnTriggerEnter(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || currentPlayer == None || Lou == None || LouIntercom == None
        Return
    EndIf

    ObjectReference playerRef = currentPlayer.GetReference()
    ObjectReference intercomRef = LouIntercom.GetReference()
    Actor louRef = Lou.GetActorReference()
    If playerRef == None || akActionRef != playerRef || intercomRef == None || louRef == None || W05_MQR_201P_LouSaysTopic_IntercomGreeting == None
        Return
    EndIf

    Int currentStage = owningQuest.GetStage()
    If currentStage < 1100 || currentStage >= 1300
        Return
    EndIf

    If !owningQuest.IsStageDone(1200)
        owningQuest.SetStage(1200)
    EndIf
    intercomRef.Say(W05_MQR_201P_LouSaysTopic_IntercomGreeting, louRef, False, playerRef)
    If !owningQuest.IsStageDone(1210)
        owningQuest.SetStage(1210)
    EndIf
    If !owningQuest.IsStageDone(1300)
        owningQuest.SetStage(1300)
    EndIf
EndEvent
