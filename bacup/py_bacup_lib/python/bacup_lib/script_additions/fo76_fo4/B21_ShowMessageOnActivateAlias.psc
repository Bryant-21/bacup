Scriptname B21_ShowMessageOnActivateAlias Extends ReferenceAlias Default

Message Property MessageToShow Auto
Bool Property ShowTraces = False Auto
Bool Property ShowIfActivePlayer = True Auto
Bool Property ShowIfStageIsSet = False Auto
Int Property StageToCheck = -1 Auto
Message[] Property ResponseMessagesToShow Auto
Int[] Property ButtonStagesToSet Auto
Int Property StageToSet = -1 Auto
Bool MessageEnabled = True

Event OnActivate(ObjectReference akActionRef)
    Quest host = GetOwningQuest()
    If !MessageEnabled || MessageToShow == None || host == None || !host.IsRunning()
        Return
    EndIf
    If ShowIfActivePlayer && akActionRef != Game.GetPlayer()
        Return
    EndIf
    If StageToCheck >= 0 && host.IsStageDone(StageToCheck) != ShowIfStageIsSet
        Return
    EndIf
    MessageEnabled = False
    If StageToSet >= 0 && !host.IsStageDone(StageToSet)
        host.SetStage(StageToSet)
    EndIf
    Int button = MessageToShow.Show()
    If button >= 0 && host.IsRunning()
        If ResponseMessagesToShow != None && button < ResponseMessagesToShow.Length && ResponseMessagesToShow[button] != None
            ResponseMessagesToShow[button].Show()
        EndIf
        If ButtonStagesToSet != None && button < ButtonStagesToSet.Length
            Int selectedStage = ButtonStagesToSet[button]
            If selectedStage > 0 && !host.IsStageDone(selectedStage)
                host.SetStage(selectedStage)
            EndIf
        EndIf
    EndIf
    MessageEnabled = True
EndEvent

Event OnAliasInit()
    MessageEnabled = True
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        MessageEnabled = True
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllEvents()
    MessageEnabled = True
EndEvent
