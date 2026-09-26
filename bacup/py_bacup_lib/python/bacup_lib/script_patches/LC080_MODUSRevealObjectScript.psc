Function PerformReveal(String animationToPlay, Bool shouldEnableLinkedrefChain, Bool shouldPlayRevealSFX)
    If animationToPlay != "" && Is3DLoaded()
        PlayAnimation(animationToPlay)
    EndIf
    If shouldPlayRevealSFX && SoundToPlayReveal != None
        SoundToPlayReveal.Play(Self)
    EndIf
    If soundToPlayLoopID > 0
        Sound.StopInstance(soundToPlayLoopID)
        soundToPlayLoopID = 0
    EndIf
    If shouldEnableLinkedrefChain && SoundToPlayLoop != None
        soundToPlayLoopID = SoundToPlayLoop.Play(Self)
    EndIf
    ObjectReference[] lights = GetLinkedRefChain(None, 100)
    Int index = 0
    While lights != None && index < lights.Length
        If lights[index] != None
            If shouldEnableLinkedrefChain
                lights[index].EnableNoWait(True)
            Else
                lights[index].DisableNoWait(True)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnInit()
    RegisterRevealEvents()
EndEvent

Function RegisterRevealEvents()
    If LC080_MODUSRevealManagerRef != None
        RegisterForCustomEvent(LC080_MODUSRevealManagerRef, "MODUSRevealStart")
        RegisterForCustomEvent(LC080_MODUSRevealManagerRef, "MODUSRevealLoad")
        RegisterForCustomEvent(LC080_MODUSRevealManagerRef, "MODUSRevealReset")
    EndIf
EndFunction

Event OnLoad()
    RegisterRevealEvents()
    If LC080_MODUSRevealManagerRef != None
        Bool completed = LC080_MODUSRevealManagerRef.IsLocalRevealCompleted(Game.GetPlayer())
        If completed
            PerformReveal(SetOpenAnim, True, False)
        Else
            PerformReveal(SetClosedAnim, False, False)
        EndIf
    EndIf
EndEvent

Event LC080_MODUSRevealManagerScript.MODUSRevealLoad(LC080_MODUSRevealManagerScript akSender, Var[] akArgs)
    If akSender != LC080_MODUSRevealManagerRef || akArgs == None || akArgs.Length < 1
        Return
    EndIf
    If akArgs[0] as Bool
        PerformReveal(SetOpenAnim, True, False)
    Else
        PerformReveal(SetClosedAnim, False, False)
    EndIf
EndEvent

Event LC080_MODUSRevealManagerScript.MODUSRevealReset(LC080_MODUSRevealManagerScript akSender, Var[] akArgs)
    If akSender == LC080_MODUSRevealManagerRef
        CancelTimer(0)
        PerformReveal(SetClosedAnim, False, False)
    EndIf
EndEvent

Event OnTimer(Int timerID)
    If timerID == 0 && Is3DLoaded()
        PerformReveal(OpenAnim, True, True)
    EndIf
EndEvent

Event OnUnload()
    CancelTimer(0)
    If soundToPlayLoopID > 0
        Sound.StopInstance(soundToPlayLoopID)
        soundToPlayLoopID = 0
    EndIf
EndEvent
