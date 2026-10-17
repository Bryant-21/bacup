; FO4 forms of the FO76 client chair:
;   - SoundCategory.Mute/UnMute take no fade argument in FO4 (MuteAudioFadeTime is unused).
;   - Actor.ShowRaceMenu is Game.ShowRaceMenu(akMenuTarget, uiMode); mode 0 is start-of-game
;     chargen. 76CharGenQuest stage 150 already opens the menu, so this chair path is only
;     the alternative owner and the quest fragment should not also call StartGameIntro.
;   - player.OnBinkMovieStoppedPlaying and SendRMIToServer are FO76-only. FO4's PlayBink
;     returns when the movie ends, so the post-video body runs directly from
;     StartGameIntro (as ContinueAfterIntro), and the server notification is dropped.
;   - Remote-event registrations take typed sources (Actor / ObjectReference) in FO4.
; @drop-member player.OnBinkMovieStoppedPlaying

Function ClientUnMuteCharGenSoundCategory()
    AudioCategorySFX76PlayerRoomDucking.UnMute()
EndFunction

Function StartGameIntro(ObjectReference akPipboyFurniture)
    Actor thePlayer = Game.GetPlayer()
    CharGenPickUpPipboyFurniture = akPipboyFurniture
    AudioCategorySFX76PlayerRoomDucking.Mute()
    Game.PlayBink("Intro.bk2", True, True, True, True, True)
    ContinueAfterIntro(thePlayer)
EndFunction

Function ContinueAfterIntro(Actor akPlayer)
    RegisterForRemoteEvent(akPlayer, "OnGetUp")
    RegisterForRemoteEvent(CharGenPickUpPipboyFurniture, "OnActivate")
    akPlayer.UnequipItem(Pipboy, False, True)
    CharGenWakeUpCameraImod.Apply(1.0)
    akPlayer.SetHasCharGenSkeleton(True)
    akPlayer.ChangeAnimFaceArchetype(None)
    Game.ShowRaceMenu(akPlayer, 0)
    UnregisterForRemoteEvent(akPlayer, "OnGetUp")
    Utility.Wait(1.0)
    OBJVault76AlarmClockRing.Play(GetLinkedRef(CharGenAlarmClock))
    akPlayer.AddKeyword(CharGenPlayerInFaceGen)
    Utility.Wait(2.0)
    HoldAtBlackImod.Remove()
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor actingActor = akActionRef as Actor
    If actingActor != None && actingActor == Game.GetPlayer()
        actingActor.EquipItem(PipboyCharGen, False, True)
        UnregisterForRemoteEvent(akSender, "OnActivate")
    EndIf
EndEvent
