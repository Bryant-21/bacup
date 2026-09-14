Scriptname B21MusicInstrumentScript extends ObjectReference

Sound Property Intro Auto Const
Sound Property Rhythm Auto Const
Sound Property Lead Auto Const
Sound Property Outro Auto Const

Actor Performer
Bool IsPlaying
Int PlaybackGeneration
Int LoopSoundInstance

Event OnActivate(ObjectReference akActionRef)
    Actor actionActor = akActionRef as Actor
    If actionActor == None || Performer != None
        Return
    EndIf

    Performer = actionActor
    RegisterForRemoteEvent(actionActor, "OnSit")
    RegisterForRemoteEvent(actionActor, "OnGetUp")
EndEvent

Event Actor.OnSit(Actor akSender, ObjectReference akFurniture)
    Int thisPlayback

    If akSender != Performer || akFurniture != Self
        Return
    EndIf

    UnregisterForRemoteEvent(akSender, "OnSit")
    IsPlaying = True
    PlaybackGeneration += 1
    thisPlayback = PlaybackGeneration

    If Intro != None
        Intro.PlayAndWait(Self)
    EndIf

    If !IsPlaying || PlaybackGeneration != thisPlayback || akSender.GetSitState() == 0
        Return
    EndIf

    PlayRhythm(thisPlayback)
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsPlaying || aiTimerID != PlaybackGeneration
        Return
    EndIf
    If Performer == None || Performer.GetSitState() == 0
        FinishPlayback(False)
        Return
    EndIf
    PlayRhythm(aiTimerID)
EndEvent

Function PlayRhythm(Int aiGeneration)
    Sound loopSound = Rhythm
    Int previousInstance = LoopSoundInstance
    Int nextInstance

    If !IsPlaying || PlaybackGeneration != aiGeneration
        Return
    EndIf
    LoopSoundInstance = 0
    If previousInstance != 0
        Sound.StopInstance(previousInstance)
    EndIf
    If loopSound == None
        loopSound = Lead
    EndIf
    If loopSound == None
        FinishPlayback(False)
        Return
    EndIf

    nextInstance = loopSound.Play(Self)
    If !IsPlaying || PlaybackGeneration != aiGeneration
        If nextInstance != 0
            Sound.StopInstance(nextInstance)
        EndIf
        Return
    EndIf
    LoopSoundInstance = nextInstance
    If nextInstance == 0
        FinishPlayback(False)
        Return
    EndIf
    ; FO76 rhythm/lead clips share a two-second musical phrase; XWM padding varies.
    StartTimer(2.0, aiGeneration)
EndFunction

Event Actor.OnGetUp(Actor akSender, ObjectReference akFurniture)
    If akSender == Performer && akFurniture == Self
        FinishPlayback(True)
    EndIf
EndEvent

Event OnUnload()
    FinishPlayback(False)
EndEvent

Function FinishPlayback(Bool abPlayOutro)
    Actor currentPerformer = Performer
    Int currentLoopSoundInstance = LoopSoundInstance

    IsPlaying = False
    CancelTimer(PlaybackGeneration)
    PlaybackGeneration += 1
    Performer = None
    LoopSoundInstance = 0

    If currentPerformer != None
        UnregisterForRemoteEvent(currentPerformer, "OnSit")
        UnregisterForRemoteEvent(currentPerformer, "OnGetUp")
    EndIf
    If currentLoopSoundInstance != 0
        Sound.StopInstance(currentLoopSoundInstance)
    EndIf
    If abPlayOutro && Outro != None
        Outro.Play(Self)
    EndIf
EndFunction
