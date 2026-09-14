Actor Function MTNS01_GetPlayer()
    Actor playerRef = None
    If currentPlayer != None
        playerRef = currentPlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function MTNS01_MoveQuestItem(ReferenceAlias akItemAlias, ReferenceAlias akDestinationAlias)
    If MoveQuestItemSpinLock || akItemAlias == None || akDestinationAlias == None
        Return
    EndIf

    ObjectReference itemRef = akItemAlias.GetReference()
    ObjectReference destinationRef = akDestinationAlias.GetReference()
    If itemRef == None || destinationRef == None || itemRef == destinationRef
        Return
    EndIf

    MoveQuestItemSpinLock = True
    itemRef.MoveTo(destinationRef)
    MoveQuestItemSpinLock = False
EndFunction

Function MTNS01_TrackRadioRepeater(ObjectReference akItemReference)
    If akItemReference != None && RadioRepeater != None && RadioRepeater.GetReference() != akItemReference
        RadioRepeater.ForceRefTo(akItemReference)
    EndIf
    MTNS01_ReconcileProgress()
EndFunction

Function MTNS01_ReconcileProgress()
    Actor playerRef = MTNS01_GetPlayer()
    If playerRef == None || !IsRunning() || IsCompleted()
        Return
    EndIf

    If IsStageDone(CraftStage) && !IsStageDone(ArrayStage) && MTNS01_RadioRepeater != None \
        && playerRef.GetItemCount(MTNS01_RadioRepeater) > 0
        SetStage(ArrayStage)
        Return
    EndIf

    If IsStageDone(GetPartsStage) && !IsStageDone(CraftStage)
        ObjectReference part01 = RadioParts01.GetReference()
        ObjectReference part02 = RadioParts02.GetReference()
        If part01 != None && part02 != None \
            && playerRef.GetItemCount(part01.GetBaseObject()) > 0 \
            && playerRef.GetItemCount(part02.GetBaseObject()) > 0
            SetStage(CraftStage)
        EndIf
    EndIf
EndFunction

Event OnQuestInit()
    Parent.OnQuestInit()
    MTNS01_ReconcileProgress()
EndEvent

Event OnQuestShutdown()
    MoveQuestItemSpinLock = False
EndEvent
