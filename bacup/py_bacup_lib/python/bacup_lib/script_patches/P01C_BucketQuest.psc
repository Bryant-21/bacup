; Bucket List photos. Once the camera is repaired (200), each PictureTriggers POI is photographed by standing in its
; trigger with the ProSnap camera drawn (FO76 opened photo mode from the camera weapon). The hidden TriggerRef member
; marks the trigger the player is inside. Every stage MinStage..MaxStage done -> CompletedStage.
Event OnQuestInit()
    PlayerRef = myPlayer.GetActorReference()
    RegisterPictureTriggers()
    UpdatePhotoCount()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200
        RegisterPictureTriggers()
    ElseIf auiStageID >= MinStage && auiStageID <= MaxStage
        UpdatePhotoCount()
    EndIf
EndEvent

Function RegisterPictureTriggers()
    If PictureTriggers == None
        Return
    EndIf
    Int index = 0
    While index < PictureTriggers.Length
        POI entry = PictureTriggers[index]
        If entry != None && entry.TriggerAlias != None
            ObjectReference poiTrigger = entry.TriggerAlias.GetReference()
            If poiTrigger != None
                RegisterForRemoteEvent(poiTrigger, "OnTriggerEnter")
                RegisterForRemoteEvent(poiTrigger, "OnTriggerLeave")
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || !IsStageDone(200) || PictureTriggers == None
        Return
    EndIf
    Int index = 0
    While index < PictureTriggers.Length
        POI entry = PictureTriggers[index]
        If entry != None && entry.TriggerAlias != None && entry.TriggerAlias.GetReference() == akSender && !IsStageDone(entry.StageToSet)
            entry.TriggerRef = akSender
            StartTimer(1.0, 1)
        EndIf
        index += 1
    EndWhile
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || PictureTriggers == None
        Return
    EndIf
    Int index = 0
    While index < PictureTriggers.Length
        POI entry = PictureTriggers[index]
        If entry != None && entry.TriggerRef == akSender
            entry.TriggerRef = None
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning() || PictureTriggers == None
        Return
    EndIf
    Bool insideActiveTrigger = False
    Bool cameraReady = IsCameraReady()
    Int index = 0
    While index < PictureTriggers.Length
        POI entry = PictureTriggers[index]
        If entry != None && entry.TriggerRef != None
            If IsStageDone(entry.StageToSet)
                entry.TriggerRef = None
            ElseIf cameraReady
                entry.TriggerRef = None
                SetStage(entry.StageToSet)
            Else
                insideActiveTrigger = True
            EndIf
        EndIf
        index += 1
    EndWhile
    If insideActiveTrigger
        StartTimer(1.0, 1)
    EndIf
EndEvent

Bool Function IsCameraReady()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf
    Weapon camera = Game.GetFormFromFile(0x46F481, "SeventySix.esm") as Weapon
    Return camera != None && PlayerRef.IsWeaponDrawn() && PlayerRef.GetEquippedWeapon() == camera
EndFunction

Function UpdatePhotoCount()
    Int count = 0
    Int stage = MinStage
    While stage <= MaxStage
        If IsStageDone(stage)
            count += 1
        EndIf
        stage += 100
    EndWhile
    PhotoCount = count
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Photos", count as Float)
    EndIf
    If count >= TotalPhotos && !IsStageDone(CompletedStage)
        SetStage(CompletedStage)
    EndIf
EndFunction
