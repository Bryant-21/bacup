Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0201_Item_00()
    If Ambient_IntroScene != None && !Ambient_IntroScene.IsPlaying()
        Ambient_IntroScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0299_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(250)
    If Ambient_IntroScene != None && Ambient_IntroScene.IsPlaying()
        Ambient_IntroScene.Stop()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    Location refugeLocation = Alias_Loc_Refuge.GetLocation()
    If playerRef == None
        Return
    EndIf

    Quest codeBlue = Game.GetFormFromFile(0x0063BED4, "SeventySix.esm") as Quest
    If codeBlue != None && CodeBlue_StartKeyword != None && !codeBlue.IsRunning() && !codeBlue.IsCompleted()
        If codeBlue.IsStopped()
            codeBlue.Reset()
        EndIf
        CodeBlue_StartKeyword.SendStoryEventAndWait(refugeLocation, playerRef, playerRef)
    EndIf

    Quest mutualAid = Game.GetFormFromFile(0x0063D5BD, "SeventySix.esm") as Quest
    If mutualAid != None && Mutual_StartKeyword != None && !mutualAid.IsRunning() && !mutualAid.IsCompleted()
        If mutualAid.IsStopped()
            mutualAid.Reset()
        EndIf
        Mutual_StartKeyword.SendStoryEventAndWait(refugeLocation, playerRef, playerRef)
    EndIf

    Quest recipeForSuccess = Game.GetFormFromFile(0x00621FB7, "SeventySix.esm") as Quest
    If recipeForSuccess != None && Recipe_StartKeyword != None && !recipeForSuccess.IsRunning() && !recipeForSuccess.IsCompleted()
        If recipeForSuccess.IsStopped()
            recipeForSuccess.Reset()
        EndIf
        Recipe_StartKeyword.SendStoryEventAndWait(refugeLocation, playerRef, playerRef)
    EndIf
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    SetObjectiveDisplayed(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(500)
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    MoveAliasToMarker(Alias_Actor_Skippy, Marker_Skippy)
    MoveAliasToMarker(Alias_Actor_Rucker, Marker_Rucker)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0804_Item_00()
    MoveAliasToMarker(Alias_Actor_Skippy, Marker_Skippy)
    MoveAliasToMarker(Alias_Actor_Rucker, Marker_Rucker)
EndFunction

Function Fragment_Stage_0807_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If FadeOut != None
        FadeOut.Apply()
    EndIf
    If Fade_Spell != None && playerRef != None
        Fade_Spell.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(800)
    If Placeholder_Tutorial != None
        Placeholder_Tutorial.Show()
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
EndFunction

Function MoveAliasToMarker(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    If actorAlias == None || markerAlias == None
        Return
    EndIf
    ObjectReference actorRef = actorAlias.GetReference()
    ObjectReference markerRef = markerAlias.GetReference()
    If actorRef != None && markerRef != None
        actorRef.MoveTo(markerRef)
    EndIf
EndFunction
