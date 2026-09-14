Event OnInit()
    myQI = GetOwningQuest()
    myPlayerRef = Alias_Player.GetActorReference()
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If myQI == None
        myQI = GetOwningQuest()
    EndIf

    If myPlayerRef == None
        myPlayerRef = Alias_Player.GetActorReference()
    EndIf

    If akActionRef != myPlayerRef || myPlayerRef != Game.GetPlayer() || myQI == None
        Return
    EndIf

    Int currentStage = myQI.GetStage()

    If currentStage >= MeetScientistsTurnOnStage && currentStage < MeetScientistsTurnOffStage
        If BS02_MQ05_Catalyst_MeetScientists != None && !BS02_MQ05_Catalyst_MeetScientists.IsPlaying()
            BS02_MQ05_Catalyst_MeetScientists.Start()
        EndIf
    ElseIf currentStage >= BlackburnBetrayalTurnOnStage && currentStage < BlackburnBetrayalTurnOffStage
        If BS02_MQ05_Catalyst_BlackburnBetrayal != None && !BS02_MQ05_Catalyst_BlackburnBetrayal.IsPlaying()
            BS02_MQ05_Catalyst_BlackburnBetrayal.Start()
        EndIf
    EndIf
EndEvent
