; Stage fragments for EN07_MQ_Death "I Am Become Death" (002D0F6B).
; The shipped FO76 client PEX is server-stripped (1982 bytes, properties and
; docstrings only). Each fragment below applies the stage's local, record-proven
; side effects and hands objective bookkeeping to EN07_IntroMiscScript, matching
; the pattern already used by QF_EN07_MQ_FleeSilo_002D0F68.

Actor Function GetLocalPlayer()
    If Alias_currentPlayer != None
        Actor aliasPlayer = Alias_currentPlayer.GetActorReference()
        If aliasPlayer != None
            Return aliasPlayer
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Function SetLocalPlayerValue(ActorValue akValue, Float afAmount)
    Actor player = GetLocalPlayer()
    If player != None && akValue != None
        player.SetValue(akValue, afAmount)
    EndIf
EndFunction

Function SayLocalModusTopic(Topic akTopic)
    If akTopic == None || Alias_MODUSRadioVoice == None
        Return
    EndIf
    ObjectReference radioVoice = Alias_MODUSRadioVoice.GetReference()
    If radioVoice != None
        radioVoice.Say(akTopic)
    EndIf
EndFunction

Function HandleStage(Int aiStage)
    Quest introQuest = Game.GetFormFromFile(0x002D0F6B, "SeventySix.esm") as Quest
    EN07_IntroMiscScript introScript = introQuest as EN07_IntroMiscScript
    If introScript != None
        introScript.HandleStage(aiStage)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    HandleStage(10)
EndFunction

Function Fragment_Stage_0015_Item_00()
    HandleStage(15)
    If EN07_MQ_IntroMisc_Scene != None && !EN07_MQ_IntroMisc_Scene.IsPlaying()
        EN07_MQ_IntroMisc_Scene.Start()
    EndIf
EndFunction

; EN07_Death_CK_TutorialState is owned by EN07_IntroMiscScript.RecountLocalTutorials,
; which mirrors the completed-tutorial count onto it. Do not write it here.
Function Fragment_Stage_0020_Item_00()
    HandleStage(20)
EndFunction

Function Fragment_Stage_0025_Item_00()
    HandleStage(25)
EndFunction

Function Fragment_Stage_0027_Item_00()
    HandleStage(27)
EndFunction

Function Fragment_Stage_0030_Item_00()
    HandleStage(30)
EndFunction

Function Fragment_Stage_0039_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_LaunchCardFound, 1.0)
    HandleStage(39)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_LaunchCardFound, 1.0)
    HandleStage(40)
EndFunction

Function Fragment_Stage_0049_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_CodePieceFound, 1.0)
    HandleStage(49)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_CodePieceFound, 1.0)
    SayLocalModusTopic(EN07_CodePieceAcquired)
    HandleStage(50)
EndFunction

Function Fragment_Stage_0059_Item_00()
    HandleStage(59)
EndFunction

Function Fragment_Stage_0060_Item_00()
    HandleStage(60)
EndFunction

Function Fragment_Stage_0065_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_PlayerSearchingForKeycard, 1.0)
    HandleStage(65)
EndFunction

Function Fragment_Stage_0067_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_PlayerSearchingForKeycard, 0.0)
    HandleStage(67)
EndFunction

Function Fragment_Stage_0069_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_FoundKeywordIntel, 1.0)
    HandleStage(69)
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetLocalPlayerValue(EN07_Death_CK_FoundKeywordIntel, 1.0)
    HandleStage(70)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetLocalPlayerValue(EN07_CompletedIntroMisc, 1.0)
    SetLocalPlayerValue(EN07_CodeHuntIntro, 1.0)
    HandleStage(100)
    If BoS01 != None && !BoS01.IsRunning() && !BoS01.IsCompleted() && BoS01_QuestStartKeyword != None
        BoS01_QuestStartKeyword.SendStoryEvent(akRef1 = GetLocalPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    HandleStage(105)
EndFunction
