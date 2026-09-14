Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    MessageEnabled = True
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If !MessageEnabled
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If ShowIfActivePlayer && akActionRef != playerRef
        Return
    EndIf

    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None || MessageToShow == None
        Return
    EndIf
    If PrereqStage >= 0 && !OwningQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If TurnOffStage >= 0 && OwningQuest.GetStage() >= TurnOffStage
        Return
    EndIf

    MessageEnabled = False
    Int buttonPressed = MessageToShow.Show()

    If ResponseMessagesToShow != None && buttonPressed >= 0 && buttonPressed < ResponseMessagesToShow.Length
        Message responseMessage = ResponseMessagesToShow[buttonPressed]
        If responseMessage != None
            responseMessage.Show()
        EndIf
    EndIf

    If ButtonStagesToSet != None && buttonPressed >= 0 && buttonPressed < ButtonStagesToSet.Length
        Int buttonStage = ButtonStagesToSet[buttonPressed]
        If buttonStage >= 0 && !OwningQuest.IsStageDone(buttonStage)
            OwningQuest.SetStage(buttonStage)
        EndIf
    EndIf
    If StageToSet >= 0 && !OwningQuest.IsStageDone(StageToSet)
        OwningQuest.SetStage(StageToSet)
    EndIf
    MessageEnabled = True
EndEvent
