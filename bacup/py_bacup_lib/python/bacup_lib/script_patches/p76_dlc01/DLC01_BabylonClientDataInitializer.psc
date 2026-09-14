; This script is pure Fallout 76 Nuclear Winter (battle royale) client plumbing. Every
; call it makes is a FO76-only Game extension for the storm-wall renderer and the
; quick-play map UI, and Fallout 4 has no counterpart for any of them:
;   Game.SetMapSpecificStormWallData, Game.RegisterQPMap,
;   Game.LoadNuclearWinterTextureMaps, Game.AddNuclearWinterWeatherTransition,
;   Game.AddNuclearWeatherNukeTransition, Game.SetNumStormFXData,
;   Game.SetStormFXData* (visual effects, spawn multipliers, distances/placement
;   modes, even-placement offsets, scales, height offsets, rotation angles,
;   fade in/out distances, instance counts/modes, fire vertex anim, sky effect,
;   pre-storm-wall alpha), Game.FinalizeSettingStormFXData, and
;   MarkQPClientDataInitializationComplete.
; BEHAVIOUR LOST (wholesale): the entire Nuclear Winter storm-wall visual system and
; its paper-map texture registration. The authored property data is preserved on the
; record; only the engine calls that consumed it are gone. OnBabylonFacadeInit, the
; FO76 engine event that drove all of this, is dropped as well.
; @drop-member OnBabylonFacadeInit

Function SetupStormWallLocationData(objectreference akMapRootObject)
EndFunction

Function LoadPaperMapTexture(objectreference akMapRootObject)
EndFunction

Function LoadNuclearWinterTextureMaps()
EndFunction

Function SetupStormWallWeatherTransitions()
EndFunction

Function SetupStormWallEffects()
EndFunction
